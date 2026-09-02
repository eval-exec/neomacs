//! Mock display test — renders fake Emacs frames via TTY or GUI.
//!
//! Usage:
//!   mock-display [OPTIONS] [DEMO]
//!
//! DEMO: default, single, hsplit, vsplit, triple, all
//!
//! OPTIONS:
//!   --gui       Render via wgpu GPU window instead of TTY
//!   --dump      Dump grid as plain text (no terminal setup)

// A mock frame-builder helper here takes many positional parameters; not worth
// restructuring for this test binary's lint gate.
#![allow(clippy::too_many_arguments)]

use neomacs_app::frontend_event::FrontendEvent;
use neomacs_display_protocol::face::{Face, FaceAttributes};
use neomacs_display_protocol::glyph_matrix::*;
use neomacs_display_protocol::types::FaceId;
use neomacs_display_protocol::types::{Color, Rect};
use neomacs_display_runtime::backend::tty::rif::TtyRif;
use neomacs_layout_engine::engine::LayoutEngine;
use neomacs_layout_engine::mock_frame::{
    MockChildFrameContent, MockFrameContent, MockStyledLine, MockWindowContent,
};
use std::collections::HashMap;
use std::io::{self, Read, Write};

// ===================================================================
// Scene: Vec<FrameDisplayState> with GUI/TTY fan-out helpers
// ===================================================================

#[derive(Clone)]
struct MockScene(Vec<FrameDisplayState>);

impl MockScene {
    fn iter(&self) -> impl Iterator<Item = &FrameDisplayState> {
        self.0.iter()
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let gui = args.iter().any(|a| a == "--gui");
    let dump = args.iter().any(|a| a == "--dump");
    let demo = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .map(|s| s.as_str())
        .unwrap_or("default");

    if gui {
        run_gui(demo);
    } else if dump {
        run_dump(demo);
    } else {
        run_tty(demo);
    }
}

// ===================================================================
// TTY backend
// ===================================================================

fn run_tty(demo: &str) {
    let (cols, rows) = query_terminal_size().unwrap_or((80, 24));
    let scene = build_demo(demo, cols, rows, 1.0, 1.0, cols as f32, rows as f32);
    let state = scene_for_tty(scene);
    setup_terminal();

    if demo == "all" {
        for name in &["default", "single", "hsplit", "vsplit", "triple"] {
            let ss = build_demo(name, cols, rows, 1.0, 1.0, cols as f32, rows as f32);
            let s = scene_for_tty(ss);
            let mut tty = TtyRif::new(cols as usize, rows as usize);
            tty.rasterize(&s);
            tty.diff_and_render();
            let out = tty.take_output();
            io::stdout().write_all(&out).unwrap();
            io::stdout().flush().unwrap();
            let label = format!("\x1b[{};1H\x1b[7m [{}] Press key \x1b[0m", rows, name);
            io::stdout().write_all(label.as_bytes()).unwrap();
            io::stdout().flush().unwrap();
            let _ = io::stdin().read(&mut [0u8]);
        }
    } else {
        let mut tty = TtyRif::new(cols as usize, rows as usize);
        tty.rasterize(&state);
        tty.diff_and_render();
        let out = tty.take_output();
        io::stdout().write_all(&out).unwrap();
        io::stdout().flush().unwrap();
        let _ = io::stdin().read(&mut [0u8]);
    }

    restore_terminal();
}

// ===================================================================
// Dump mode
// ===================================================================

fn run_dump(demo: &str) {
    let (cols, rows) = query_terminal_size().unwrap_or((80, 24));
    let scene = build_demo(demo, cols, rows, 1.0, 1.0, cols as f32, rows as f32);
    let state = scene_for_tty(scene);
    let mut tty = TtyRif::new(cols as usize, rows as usize);
    tty.rasterize(&state);
    for (i, line) in tty.dump_desired().iter().enumerate() {
        println!("{:>2}: |{}|", i, line.trim_end());
    }
}

// ===================================================================
// GUI backend
// ===================================================================

fn run_gui(demo: &str) {
    use neomacs_display_runtime::render_thread::{
        RenderThread, SharedImageRenderState, SharedMonitorInfo,
    };
    use neomacs_display_runtime::thread_comm::{
        InputEvent, LifecycleCommand, RenderCommand, ThreadComms,
    };
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    let _logging_guard = neovm_core::logging::init(neovm_core::logging::LogTarget::Stdout);

    let mut engine = LayoutEngine::new();
    engine.enable_cosmic_metrics();
    let backend = neomacs_layout_engine::font_backend::default_font_backend();
    let family = backend.resolve_family("monospace");
    // Use the same physical pixel size that layout_mock_frame will
    // derive from the default face's point-size conversion.
    // Otherwise window pixel_bounds won't match the font metrics used
    // during layout, causing mode-lines and the minibuffer to be
    // misplaced or clipped.
    let sizing = neomacs_layout_engine::font::sizing::FontSizing::native_gui();
    let physical_size =
        neomacs_layout_engine::font::sizing::points_to_layout_pixels(10.0, sizing.layout_dpi());
    let char_w = {
        let fm = engine.font_metrics.as_mut().unwrap();
        fm.char_width('m', &family, 400, false, physical_size)
            .max(1.0)
    };
    let char_h = {
        let fm = engine.font_metrics.as_mut().unwrap();
        fm.font_metrics(&family, 400, false, physical_size)
            .line_height
            .max(1.0)
    };
    // Size the frame to fit on a typical screen.
    let cols = (2400.0 / char_w).floor().max(80.0) as u16;
    let rows = (1600.0 / char_h).floor().max(40.0) as u16;
    tracing::info!(
        "mock-display gui: physical_size={:.1} family={} char_w={:.1} char_h={:.1}",
        physical_size,
        family,
        char_w,
        char_h
    );
    let pixel_w = (cols as f32 * char_w) as u32;
    let pixel_h = (rows as f32 * char_h) as u32;
    tracing::info!(
        "mock-display gui: cols={} rows={} pixel_w={} pixel_h={}",
        cols,
        rows,
        pixel_w,
        pixel_h
    );

    let comms = ThreadComms::new();
    let (emacs_comms, render_comms) = comms.split();

    let image_dims: SharedImageRenderState =
        Arc::new(neomacs_display_runtime::render_thread::ImageRenderState::default());
    let shared_monitors: SharedMonitorInfo =
        Arc::new((Mutex::new(Vec::new()), std::sync::Condvar::new()));

    let render_thread = RenderThread::spawn(
        render_comms,
        pixel_w,
        pixel_h,
        format!("Neomacs Mock — {}", demo),
        Arc::clone(&image_dims),
        Arc::clone(&shared_monitors),
    )
    .unwrap_or_else(|err| {
        eprintln!("Failed to start render thread: {err}");
        std::process::exit(1);
    });

    eprintln!(
        "GUI mock: {}x{} px ({}x{} cells @ {}x{} font), demo={}",
        pixel_w, pixel_h, cols, rows, char_w, char_h, demo
    );

    let scene = build_demo(
        demo,
        cols,
        rows,
        char_w,
        char_h,
        pixel_w as f32,
        pixel_h as f32,
    );
    for (index, s) in scene.iter().enumerate() {
        let mut state = s.clone();
        state.presentation_id = neomacs_display_protocol::PresentationId::new(index as u64 + 1);
        let placement = state.frame_placement;
        state.frame_placement = neomacs_display_protocol::PresentedFramePlacement::new(
            placement.frame(),
            state.presentation_id,
            placement.parent(),
            placement.outer_in_parent(),
            placement.z_order(),
        );
        state.presented_hit_index = neomacs_display_protocol::PresentedHitIndex::from_parts(
            state.presentation_id,
            vec![],
            vec![],
        )
        .expect("mock presentation has valid empty hit geometry");
        let sealed = neomacs_display_protocol::SealedFramePresentation::seal(state)
            .expect("mock GUI frame is a coherent presentation");
        let _ = emacs_comms.frame_tx.send(sealed);
    }

    loop {
        std::thread::sleep(Duration::from_millis(100));
        while let Ok(event) = emacs_comms.input_rx.try_recv() {
            if let InputEvent::Frontend(FrontendEvent::Key(key)) = event
                && (key.symbol().get() == b'q' as u32 || key.symbol().get() == 0xff1b)
            {
                let _ = emacs_comms
                    .cmd_tx
                    .send(RenderCommand::Lifecycle(LifecycleCommand::Shutdown));
                render_thread.join();
                return;
            }
        }
    }
}

// ===================================================================
// Scene utilities
// ===================================================================

fn scene_for_tty(scene: MockScene) -> FrameDisplayState {
    let mut iter = scene.0.into_iter();
    let mut main = iter.next().expect("Scene must have a main frame");
    for child in iter {
        let outer = child.frame_placement.outer_in_parent();
        let ox = outer.x();
        let oy = outer.y();
        for mut entry in child.window_matrices {
            entry.pixel_bounds.x += ox;
            entry.pixel_bounds.y += oy;
            main.window_matrices.push(entry);
        }
        for mut bg in child.backgrounds {
            bg.bounds.x += ox;
            bg.bounds.y += oy;
            main.backgrounds.push(bg);
        }
        for mut border in child.borders {
            border.x += ox;
            border.y += oy;
            main.borders.push(border);
        }
        for (id, face) in &child.faces {
            main.faces.entry(*id).or_insert_with(|| face.clone());
        }
    }
    main
}

fn build_demo(
    name: &str,
    cols: u16,
    rows: u16,
    char_w: f32,
    char_h: f32,
    pixel_w: f32,
    pixel_h: f32,
) -> MockScene {
    let faces = build_faces();
    let content = match name {
        "default" => build_default(cols, rows, char_w, char_h, pixel_w, pixel_h, &faces),
        "hsplit" => build_hsplit(cols, rows, char_w, char_h, pixel_w, pixel_h, &faces),
        "vsplit" => build_vsplit(cols, rows, char_w, char_h, pixel_w, pixel_h, &faces),
        "triple" => build_triple(cols, rows, char_w, char_h, pixel_w, pixel_h, &faces),
        _ => build_single(cols, rows, char_w, char_h, pixel_w, pixel_h, &faces),
    };
    let mut engine = LayoutEngine::new();
    engine.enable_cosmic_metrics();
    let states = engine.layout_mock_frame(&content, char_w, char_h);
    MockScene(states)
}

// ===================================================================
// Buffer content
// ===================================================================

fn scratch_buffer_lines() -> Vec<(&'static str, FaceId)> {
    vec![
        (";; This is the *scratch* buffer.", FaceId::new(5)),
        ("", FaceId::new(0)),
        ("(defun hello (name)", FaceId::new(3)),
        ("  \"Say hello to NAME.\"", FaceId::new(4)),
        ("  (message \"Hello, %s!\" name))", FaceId::new(3)),
        ("", FaceId::new(0)),
        (";; Type C-x C-e to evaluate", FaceId::new(12)),
        ("", FaceId::new(0)),
        ("(setq neomacs-version \"0.1.0\")", FaceId::new(0)),
        ("(setq display-pipeline 'glyph-matrix)", FaceId::new(0)),
        ("", FaceId::new(0)),
        (
            ";; GNU Emacs compatible glyph matrix model",
            FaceId::new(12),
        ),
        (";; TTY rendering via TtyRif", FaceId::new(12)),
        (
            ";; Single-thread, no channel, matching GNU",
            FaceId::new(12),
        ),
        ("", FaceId::new(0)),
        ("", FaceId::new(0)),
        ("", FaceId::new(0)),
        ("", FaceId::new(0)),
        ("  C-x C-e  ", FaceId::new(8)),
        ("", FaceId::new(0)),
        ("", FaceId::new(0)),
        ("", FaceId::new(0)),
    ]
}

fn messages_buffer_lines() -> Vec<(&'static str, FaceId)> {
    vec![
        ("Loading /usr/share/emacs/site-lisp/...", FaceId::new(0)),
        (
            "For information about GNU Emacs, type C-h C-a.",
            FaceId::new(0),
        ),
        ("Starting new Emacs daemon...", FaceId::new(0)),
        ("Loaded custom theme 'modus-vivendi'", FaceId::new(4)),
        ("Loading org-mode...done", FaceId::new(0)),
        ("Mark set", FaceId::new(5)),
        ("Quit", FaceId::new(3)),
        ("Buffer is read-only: *Messages*", FaceId::new(3)),
    ]
}

fn help_buffer_lines() -> Vec<(&'static str, FaceId)> {
    vec![
        ("GNU Emacs Manual", FaceId::new(3)),
        ("================", FaceId::new(3)),
        ("", FaceId::new(0)),
        ("  Emacs is the extensible,", FaceId::new(0)),
        ("  customizable, self-documenting", FaceId::new(0)),
        ("  real-time display editor.", FaceId::new(0)),
        ("", FaceId::new(0)),
        (";; Key Bindings:", FaceId::new(12)),
        ("  C-x C-f  Find file", FaceId::new(0)),
        ("  C-x C-s  Save file", FaceId::new(0)),
        ("  C-x b    Switch buffer", FaceId::new(0)),
        ("  C-x 2    Split horizontal", FaceId::new(0)),
        ("  C-x 3    Split vertical", FaceId::new(0)),
        ("  C-x 0    Delete window", FaceId::new(0)),
        ("  C-x 1    Delete other windows", FaceId::new(0)),
        ("  C-g      Keyboard quit", FaceId::new(0)),
    ]
}

// ===================================================================
// Layout builders — produce MockFrameContent (the evaluator handoff)
// ===================================================================

fn build_single(
    _cols: u16,
    rows: u16,
    _char_w: f32,
    char_h: f32,
    pixel_w: f32,
    pixel_h: f32,
    faces: &HashMap<FaceId, Face>,
) -> MockFrameContent {
    let r = rows as usize;
    let text_rows = r - 2;
    let scratch: Vec<MockStyledLine> = scratch_buffer_lines()
        .into_iter()
        .map(|(t, f)| MockStyledLine::from_str(t, f))
        .collect();
    MockFrameContent {
        frame_id: 1,
        faces: faces.values().cloned().collect(),
        windows: vec![MockWindowContent {
            window_id: 1,
            lines: scratch,
            mode_line: MockStyledLine::from_str(
                " -:**-  *scratch*      Top L1     (Lisp Interaction)",
                FaceId::new(1),
            ),
            pixel_bounds: Rect::new(0.0, 0.0, pixel_w, (text_rows + 1) as f32 * char_h),
            selected: true,
            truncated_lines: false,
        }],
        child_frames: vec![],
        frame_pixel_width: pixel_w,
        frame_pixel_height: pixel_h,
        background: Color::new(0.0, 0.0, 0.0, 1.0),
        minibuffer: None,
        menu_bar: None,
    }
}

fn build_hsplit(
    _cols: u16,
    rows: u16,
    _char_w: f32,
    char_h: f32,
    pixel_w: f32,
    _pixel_h: f32,
    faces: &HashMap<FaceId, Face>,
) -> MockFrameContent {
    let r = rows as usize;
    let half = (r - 1) / 2;
    let scratch: Vec<MockStyledLine> = scratch_buffer_lines()
        .into_iter()
        .map(|(t, f)| MockStyledLine::from_str(t, f))
        .collect();
    let messages: Vec<MockStyledLine> = messages_buffer_lines()
        .into_iter()
        .map(|(t, f)| MockStyledLine::from_str(t, f))
        .collect();
    MockFrameContent {
        frame_id: 1,
        faces: faces.values().cloned().collect(),
        windows: vec![
            MockWindowContent {
                window_id: 1,
                lines: scratch,
                mode_line: MockStyledLine::from_str(
                    " -:**-  *scratch*      Top L1     (Lisp Interaction)",
                    FaceId::new(1),
                ),
                pixel_bounds: Rect::new(0., 0., pixel_w, half as f32 * char_h),
                selected: true,
                truncated_lines: false,
            },
            MockWindowContent {
                window_id: 2,
                lines: messages,
                mode_line: MockStyledLine::from_str(
                    " -:---  *Messages*     Bot L1     (Messages)",
                    FaceId::new(1),
                ),
                pixel_bounds: Rect::new(
                    0.,
                    half as f32 * char_h,
                    pixel_w,
                    (r - 1 - half) as f32 * char_h,
                ),
                selected: true,
                truncated_lines: false,
            },
        ],
        child_frames: vec![],
        frame_pixel_width: pixel_w,
        frame_pixel_height: r as f32 * char_h,
        background: Color::new(0.0, 0.0, 0.0, 1.0),
        minibuffer: None,
        menu_bar: None,
    }
}

fn build_vsplit(
    cols: u16,
    rows: u16,
    char_w: f32,
    char_h: f32,
    pixel_w: f32,
    _pixel_h: f32,
    faces: &HashMap<FaceId, Face>,
) -> MockFrameContent {
    let c = cols as usize;
    let r = rows as usize;
    let left_cols = c / 2;
    let right_cols = c - left_cols - 1;
    let text_rows = r - 2;
    let scratch: Vec<MockStyledLine> = scratch_buffer_lines()
        .into_iter()
        .map(|(t, f)| MockStyledLine::from_str(t, f))
        .collect();
    let help: Vec<MockStyledLine> = help_buffer_lines()
        .into_iter()
        .map(|(t, f)| MockStyledLine::from_str(t, f))
        .collect();
    let ml_left = format!(
        " -:**-  *scratch*{:>w$}",
        "",
        w = left_cols.saturating_sub(17)
    );
    let ml_right = format!(
        " -:---  help.el{:>w$}",
        "",
        w = right_cols.saturating_sub(15)
    );
    MockFrameContent {
        frame_id: 1,
        faces: faces.values().cloned().collect(),
        windows: vec![
            MockWindowContent {
                window_id: 1,
                lines: scratch,
                mode_line: MockStyledLine::from_str(
                    &format!("{}|{}", ml_left, ml_right),
                    FaceId::new(1),
                ),
                pixel_bounds: Rect::new(
                    0.,
                    0.,
                    left_cols as f32 * char_w,
                    (text_rows + 1) as f32 * char_h,
                ),
                selected: true,
                truncated_lines: false,
            },
            MockWindowContent {
                window_id: 2,
                lines: help,
                mode_line: MockStyledLine::from_str("", FaceId::new(1)),
                pixel_bounds: Rect::new(
                    (left_cols + 1) as f32 * char_w,
                    0.,
                    right_cols as f32 * char_w,
                    (text_rows + 1) as f32 * char_h,
                ),
                selected: true,
                truncated_lines: false,
            },
        ],
        child_frames: vec![],
        frame_pixel_width: pixel_w,
        frame_pixel_height: r as f32 * char_h,
        background: Color::new(0.0, 0.0, 0.0, 1.0),
        minibuffer: None,
        menu_bar: None,
    }
}

fn build_triple(
    cols: u16,
    rows: u16,
    char_w: f32,
    char_h: f32,
    pixel_w: f32,
    _pixel_h: f32,
    faces: &HashMap<FaceId, Face>,
) -> MockFrameContent {
    let c = cols as usize;
    let r = rows as usize;
    let left_cols = c / 2;
    let right_cols = c - left_cols - 1;
    let right_half = (r - 1) / 2;
    let rx = (left_cols + 1) as f32 * char_w;
    let scratch: Vec<MockStyledLine> = scratch_buffer_lines()
        .into_iter()
        .map(|(t, f)| MockStyledLine::from_str(t, f))
        .collect();
    let messages: Vec<MockStyledLine> = messages_buffer_lines()
        .into_iter()
        .map(|(t, f)| MockStyledLine::from_str(t, f))
        .collect();
    let help: Vec<MockStyledLine> = help_buffer_lines()
        .into_iter()
        .map(|(t, f)| MockStyledLine::from_str(t, f))
        .collect();
    MockFrameContent {
        frame_id: 1,
        faces: faces.values().cloned().collect(),
        windows: vec![
            MockWindowContent {
                window_id: 1,
                lines: scratch,
                mode_line: MockStyledLine::from_str(
                    " -:**-  *scratch*      (Lisp Interaction)",
                    FaceId::new(1),
                ),
                pixel_bounds: Rect::new(0., 0., left_cols as f32 * char_w, (r - 2) as f32 * char_h),
                selected: true,
                truncated_lines: false,
            },
            MockWindowContent {
                window_id: 2,
                lines: messages,
                mode_line: MockStyledLine::from_str(
                    " -:---  *Messages*     (Messages)",
                    FaceId::new(1),
                ),
                pixel_bounds: Rect::new(
                    rx,
                    0.,
                    right_cols as f32 * char_w,
                    right_half as f32 * char_h,
                ),
                selected: true,
                truncated_lines: false,
            },
            MockWindowContent {
                window_id: 3,
                lines: help,
                mode_line: MockStyledLine::from_str(
                    " -:---  *Help*         (Help)",
                    FaceId::new(1),
                ),
                pixel_bounds: Rect::new(
                    rx,
                    right_half as f32 * char_h,
                    right_cols as f32 * char_w,
                    (r - 1 - right_half) as f32 * char_h,
                ),
                selected: true,
                truncated_lines: false,
            },
        ],
        child_frames: vec![],
        frame_pixel_width: pixel_w,
        frame_pixel_height: r as f32 * char_h,
        background: Color::new(0.0, 0.0, 0.0, 1.0),
        minibuffer: None,
        menu_bar: None,
    }
}

fn build_default(
    cols: u16,
    rows: u16,
    char_w: f32,
    char_h: f32,
    pixel_w: f32,
    _pixel_h: f32,
    faces: &HashMap<FaceId, Face>,
) -> MockFrameContent {
    let c = cols as usize;
    let r = rows as usize;
    let top_half = (r - 1) / 2;
    let bot_text = r - 1 - top_half;
    let left_cols = c / 2;
    let right_cols = c - left_cols - 1;
    let top_text = top_half - 1;
    let rx = (left_cols + 1) as f32 * char_w;

    let scratch: Vec<MockStyledLine> = scratch_buffer_lines()
        .into_iter()
        .map(|(t, f)| MockStyledLine::from_str(t, f))
        .collect();
    let messages: Vec<MockStyledLine> = messages_buffer_lines()
        .into_iter()
        .map(|(t, f)| MockStyledLine::from_str(t, f))
        .collect();
    let help: Vec<MockStyledLine> = help_buffer_lines()
        .into_iter()
        .map(|(t, f)| MockStyledLine::from_str(t, f))
        .collect();

    // Child-frame: 60% of top-right window, centered
    let cf_cols = ((right_cols as f32 * 0.6) as usize).max(20);
    let cf_w = cf_cols as f32 * char_w;
    let cf_x = rx + (right_cols as f32 - cf_cols as f32) * 0.5 * char_w;
    let cf_rows = ((top_text as f32 * 0.6) as usize).max(6);
    let cf_h = (cf_rows as f32 + 2.0) * char_h;
    let cf_y = (top_text as f32 - (cf_rows as f32 + 2.0)) * 0.5 * char_h;
    let title_str = format!(" {:-<w$}", "Completions ", w = cf_cols.saturating_sub(1));
    let mut cf_lines = vec![MockStyledLine::from_str(
        &" ".repeat(cf_cols),
        FaceId::new(9),
    )];
    cf_lines.push(MockStyledLine::from_str(&title_str, FaceId::new(11)));
    let items = [
        "  describe-function     ",
        "  describe-variable     ",
        "\u{25b8} describe-symbol        ",
        "  describe-key          ",
        "  describe-mode         ",
        "  describe-char         ",
        "  describe-face         ",
        "  describe-coding-system",
        "  describe-bindings     ",
        "  describe-package      ",
    ];
    for (i, item) in items.iter().enumerate() {
        cf_lines.push(MockStyledLine::from_str(
            item,
            if i == 2 {
                FaceId::new(10)
            } else {
                FaceId::new(9)
            },
        ));
    }

    MockFrameContent {
        frame_id: 1,
        faces: faces.values().cloned().collect(),
        windows: vec![
            MockWindowContent {
                window_id: 1,
                lines: scratch,
                mode_line: MockStyledLine::from_str(
                    " -:**-  *scratch*      (Lisp Interaction)",
                    FaceId::new(1),
                ),
                pixel_bounds: Rect::new(
                    0.,
                    0.,
                    left_cols as f32 * char_w,
                    (top_text + 1) as f32 * char_h,
                ),
                selected: true,
                truncated_lines: false,
            },
            MockWindowContent {
                window_id: 2,
                lines: messages,
                mode_line: MockStyledLine::from_str(
                    " -:---  *Messages*     (Messages)",
                    FaceId::new(1),
                ),
                pixel_bounds: Rect::new(
                    rx,
                    0.,
                    right_cols as f32 * char_w,
                    (top_text + 1) as f32 * char_h,
                ),
                selected: false,
                truncated_lines: false,
            },
            MockWindowContent {
                window_id: 3,
                lines: help,
                mode_line: MockStyledLine::from_str(
                    " -:---  *Help*         (Help)",
                    FaceId::new(1),
                ),
                pixel_bounds: Rect::new(
                    0.,
                    top_half as f32 * char_h,
                    pixel_w,
                    (bot_text - 1) as f32 * char_h + 1.0 * char_h,
                ),
                selected: false,
                truncated_lines: false,
            },
        ],
        child_frames: vec![MockChildFrameContent {
            frame_id: 100,
            window: MockWindowContent {
                window_id: 1,
                lines: cf_lines,
                mode_line: MockStyledLine::from_str("", FaceId::new(1)),
                pixel_bounds: Rect::new(0., 0., cf_w, cf_h),
                selected: false,
                truncated_lines: false,
            },
            parent_x: cf_x,
            parent_y: cf_y,
            z_order: 1,
        }],
        minibuffer: Some(MockWindowContent {
            window_id: 999,
            lines: vec![MockStyledLine::from_str(
                "For information about GNU Emacs and the GNU system, type C-h C-a.",
                FaceId::new(0),
            )],
            mode_line: MockStyledLine::from_str("", FaceId::new(0)),
            pixel_bounds: Rect::new(0., (r - 1) as f32 * char_h, pixel_w, 1.0 * char_h),
            selected: false,
            truncated_lines: false,
        }),
        frame_pixel_width: pixel_w,
        frame_pixel_height: r as f32 * char_h,
        background: Color::new(0.0, 0.0, 0.0, 1.0),
        menu_bar: None,
    }
}

// ===================================================================
// Faces
// ===================================================================

fn build_faces() -> HashMap<FaceId, Face> {
    use neomacs_display_protocol::gradient::{ColorStop, Gradient};

    let mut f = HashMap::new();
    f.insert(
        FaceId::new(0),
        mk(
            FaceId::new(0),
            0.87,
            0.87,
            0.87,
            0.0,
            0.0,
            0.0,
            400,
            false,
            None,
        ),
    );

    // Face 1: Mode-line with noise gradient, black foreground
    let mode_line_gradient = Some(Box::new(Gradient::Noise {
        scale: 4.0,
        octaves: 4,
        color1: Color::new(1.0, 0.42, 0.62, 1.0), // #FF6B9D
        color2: Color::new(1.0, 0.95, 0.97, 1.0), // #FFF2F7
    }));
    f.insert(
        FaceId::new(1),
        mk(
            FaceId::new(1),
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            700,
            false,
            mode_line_gradient,
        ),
    );

    // Face 2: Line numbers — gutter style
    f.insert(
        FaceId::new(2),
        mk(
            FaceId::new(2),
            0.2,
            0.65,
            0.75,
            0.0,
            0.04,
            0.06,
            300,
            true,
            None,
        ),
    );

    // Face 3: Comments with radial gradient
    let comment_gradient = Some(Box::new(Gradient::Radial {
        center_x: 0.5,
        center_y: 0.5,
        radius: 0.8,
        stops: vec![
            ColorStop::new(0.0, Color::new(1.0, 1.0, 1.0, 1.0)),
            ColorStop::new(1.0, Color::new(0.0, 0.2, 0.4, 1.0)),
        ],
    }));
    f.insert(
        FaceId::new(3),
        mk(
            FaceId::new(3),
            1.0,
            0.6,
            0.2,
            0.0,
            0.0,
            0.0,
            700,
            false,
            comment_gradient,
        ),
    );

    // Face 4: Strings with conic gradient
    let string_gradient = Some(Box::new(Gradient::Conic {
        center_x: 0.5,
        center_y: 0.5,
        angle_offset: 0.0,
        stops: vec![
            ColorStop::new(0.00, Color::new(1.0, 0.0, 0.0, 1.0)),
            ColorStop::new(0.17, Color::new(1.0, 0.5, 0.0, 1.0)),
            ColorStop::new(0.33, Color::new(1.0, 1.0, 0.0, 1.0)),
            ColorStop::new(0.50, Color::new(0.0, 1.0, 0.0, 1.0)),
            ColorStop::new(0.67, Color::new(0.0, 0.0, 1.0, 1.0)),
            ColorStop::new(0.83, Color::new(0.3, 0.0, 0.5, 1.0)),
            ColorStop::new(1.00, Color::new(1.0, 0.0, 0.0, 1.0)),
        ],
    }));
    f.insert(
        FaceId::new(4),
        mk(
            FaceId::new(4),
            0.4,
            0.9,
            0.4,
            0.0,
            0.0,
            0.0,
            400,
            false,
            string_gradient,
        ),
    );

    f.insert(
        FaceId::new(5),
        mk(
            FaceId::new(5),
            0.4,
            0.7,
            0.7,
            0.0,
            0.0,
            0.0,
            400,
            true,
            None,
        ),
    );
    f.insert(
        FaceId::new(6),
        mk(
            FaceId::new(6),
            0.87,
            0.87,
            0.87,
            0.15,
            0.15,
            0.15,
            400,
            false,
            None,
        ),
    );
    f.insert(
        FaceId::new(7),
        mk(
            FaceId::new(7),
            0.4,
            0.4,
            0.4,
            0.0,
            0.0,
            0.0,
            400,
            false,
            None,
        ),
    );

    // Face 8: Rounded box for key bindings — gray bg, black fg, gold border
    {
        let mut box_face = Face::new(FaceId::new(8));
        box_face.foreground = Color::new(0.0, 0.0, 0.0, 1.0);
        box_face.background = Color::new(0.3, 0.3, 0.3, 1.0);
        box_face.font_weight = 400;
        box_face.box_type = neomacs_display_protocol::face::BoxType::Line;
        box_face.box_line_width = 2.into();
        box_face.box_corner_radius = 8;
        box_face.box_color = Some(Color::new(1.0, 0.84, 0.0, 1.0));
        f.insert(FaceId::new(8), box_face);
    }

    // Faces 9-11: Child-frame backgrounds
    f.insert(
        FaceId::new(9),
        mk(
            FaceId::new(9),
            0.9,
            0.9,
            0.95,
            0.08,
            0.08,
            0.14,
            400,
            false,
            None,
        ),
    );
    f.insert(
        FaceId::new(10),
        mk(
            FaceId::new(10),
            0.9,
            0.9,
            0.95,
            0.18,
            0.22,
            0.38,
            400,
            false,
            None,
        ),
    );
    f.insert(
        FaceId::new(11),
        mk(
            FaceId::new(11),
            0.9,
            0.9,
            0.95,
            0.15,
            0.20,
            0.35,
            400,
            false,
            None,
        ),
    );

    // Face 12: Comments — warm orange-red italic
    f.insert(
        FaceId::new(12),
        mk(
            FaceId::new(12),
            1.0,
            0.5,
            0.3,
            0.0,
            0.0,
            0.0,
            400,
            true,
            None,
        ),
    );
    f
}

fn mk(
    id: FaceId,
    fr: f32,
    fg: f32,
    fb: f32,
    br: f32,
    _bg: f32,
    bb: f32,
    weight: u16,
    italic: bool,
    gradient: Option<Box<neomacs_display_protocol::gradient::Gradient>>,
) -> Face {
    let mut attrs = FaceAttributes::empty();
    if italic {
        attrs |= FaceAttributes::ITALIC;
    }
    let mut face = Face::new(id);
    face.foreground = Color::new(fr, fg, fb, 1.0);
    face.background = Color::new(br, _bg, bb, 1.0);
    face.font_size = 10.0;
    face.font_weight = weight;
    face.attributes = attrs;
    face.background_gradient = gradient;
    face
}

// ===================================================================
// Terminal helpers (cross-platform via crossterm)
// ===================================================================

fn setup_terminal() {
    let _ = crossterm::terminal::enable_raw_mode();
    print!("\x1b[?1049h\x1b[?25l\x1b[2J\x1b[H");
    io::stdout().flush().unwrap();
}

fn restore_terminal() {
    print!("\x1b[?25h\x1b[?1049l");
    io::stdout().flush().unwrap();
    let _ = crossterm::terminal::disable_raw_mode();
}

fn query_terminal_size() -> Option<(u16, u16)> {
    crossterm::terminal::size().ok()
}
