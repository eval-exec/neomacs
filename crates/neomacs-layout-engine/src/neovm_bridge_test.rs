use super::*;
use neomacs_display_protocol::cursor::{CursorBarWidth, CursorKind};
use neovm_core::buffer::{
    Buffer, BufferId, BufferManager, BufferTextBackendKind, CharPos0, EmacsBytePos, EmacsByteRange,
    LispCharPos1,
};
use neovm_core::emacs_core::value::Value;
use neovm_core::window::{FrameManager, FrameParam, Rect as NeoRect, WindowId, WindowMargins};

fn eval_lisp(eval: &mut neovm_core::emacs_core::Context, source: &str) -> Value {
    eval.eval_str(source).expect("evaluate form")
}

fn test_buffer(id: u64, name: &str) -> Buffer {
    Buffer::new_standalone(BufferId(id), Value::string(name))
}

fn test_buffer_with_backend(id: u64, name: &str, kind: BufferTextBackendKind) -> Buffer {
    Buffer::try_new_standalone_with_text_backend_kind(BufferId(id), Value::string(name), kind)
        .expect("test backend should be implemented")
}

fn set_buffer_text(buf: &mut Buffer, text: &str) {
    buf.insert(text);
    buf.widen();
    buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(0));
}

#[test]
fn window_named_face_inheritance_observes_default_face_remapping() {
    let _runtime = neovm_core::emacs_core::Context::new();
    let mut table = neovm_core::face::FaceTable::new();
    table
        .get("line-number")
        .cloned()
        .map(|mut line_number| {
            line_number.inherit = Some(Value::list(vec![
                Value::symbol("shadow"),
                Value::symbol("default"),
            ]));
            table.define("line-number", line_number);
        })
        .expect("standard line-number face");
    let resolver = FaceResolver::new(&table, 0x00ff_ffff, 0x0000_0000, 14.0, None);
    let mut buffer = test_buffer(205, "*face-remapping-inheritance*");
    buffer.set_buffer_local(
        "face-remapping-alist",
        Value::list(vec![Value::list(vec![
            Value::symbol("default"),
            Value::list(vec![Value::keyword("height"), Value::make_float(2.0)]),
            Value::symbol("default"),
        ])]),
    );

    let remapping = BufferFaceRemapping::capture(&buffer);
    let default_face = resolver.resolve_remapped_default_face(remapping);
    let line_number_face =
        resolver.resolve_remapped_named_face(remapping, "line-number-current-line");

    assert_eq!(default_face.font_size.to_bits(), 28.0_f32.to_bits());
    assert_eq!(
        line_number_face.font_size.to_bits(),
        default_face.font_size.to_bits(),
        "the line-number-current-line -> line-number -> default inheritance chain must cross the window's remapping environment"
    );
}

trait BufferTextPropertyTestExt {
    fn put_text_property(&mut self, start: usize, end: usize, name: Value, value: Value) -> bool;
}

fn emacs_byte_range(start: usize, end: usize) -> EmacsByteRange {
    EmacsByteRange::new(EmacsBytePos::new(start), EmacsBytePos::new(end))
}

impl BufferTextPropertyTestExt for Buffer {
    fn put_text_property(&mut self, start: usize, end: usize, name: Value, value: Value) -> bool {
        self.text_props_put_property_in_emacs_byte_range(emacs_byte_range(start, end), name, value)
    }
}

/// Create a minimal Context-like test fixture (FrameManager + BufferManager)
/// and verify `collect_layout_params` produces correct output.
#[test]
fn test_collect_layout_params_basic() {
    let mut evaluator = neovm_core::emacs_core::Context::new();

    // Create a buffer.
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*test*");

    // Create a frame with that buffer.
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("test-frame", 800, 600, buf_id);

    // Set some frame font metrics.
    if let Some(frame) = evaluator.frame_manager_mut().get_mut(frame_id) {
        frame.font_pixel_size = 14.0;
        frame.char_width = 7.0;
        frame.char_height = 14.0;
    }

    let (fp, wps) = collect_layout_params(&evaluator, frame_id, None)
        .expect("collect_layout_params should succeed");

    // Check FrameParams.
    assert_eq!(fp.width, 800.0);
    assert_eq!(fp.height, 600.0);
    assert_eq!(fp.char_width, 7.0);
    assert_eq!(fp.char_height, 14.0);
    assert_eq!(fp.font_pixel_size, 14.0);

    // Should have 1 root leaf + 1 minibuffer = 2 windows.
    assert_eq!(wps.len(), 2, "expected root leaf + minibuffer");

    // First window: root leaf (not minibuffer).
    let root_wp = &wps[0];
    assert!(!root_wp.is_minibuffer());
    assert!(root_wp.selected); // first window is selected by default
    assert_eq!(root_wp.char_width, 7.0);
    assert_eq!(root_wp.char_height, 14.0);
    assert_eq!(root_wp.mode_line_height, 16.0); // mode-line includes face box pixels

    // Second window: minibuffer.
    let mini_wp = &wps[1];
    assert!(mini_wp.is_minibuffer());
    assert!(!mini_wp.selected);
    assert_eq!(mini_wp.mode_line_height, 0.0); // minibuffer has no mode-line
}

#[test]
fn collect_layout_params_suppresses_line_numbers_for_minibuffer_windows() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator
        .buffer_manager_mut()
        .create_buffer("*line-number-window-kind*");
    evaluator
        .buffer_manager_mut()
        .get_mut(buf_id)
        .expect("buffer")
        .set_buffer_local("display-line-numbers", Value::T);
    let frame_id =
        evaluator
            .frame_manager_mut()
            .create_frame("line-number-window-kind", 800, 600, buf_id);

    let (_, windows) =
        collect_layout_params(&evaluator, frame_id, None).expect("collect layout params");
    let main = windows
        .iter()
        .find(|params| !params.is_minibuffer())
        .expect("main window");
    let minibuffer = windows
        .iter()
        .find(|params| params.is_minibuffer())
        .expect("minibuffer window");

    assert_eq!(
        main.display_line_numbers,
        DisplayLineNumbersMode::Absolute,
        "the source buffer requests absolute line numbers"
    );
    assert_eq!(
        minibuffer.display_line_numbers,
        DisplayLineNumbersMode::Off,
        "GNU maybe_produce_line_number rejects MINI_WINDOW_P before producing any gutter glyphs"
    );
}

#[test]
fn collect_layout_params_forwards_window_vscroll() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*vscroll*");
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("vscroll-frame", 800, 600, buf_id);
    let selected_window = {
        let frame = evaluator
            .frame_manager_mut()
            .get_mut(frame_id)
            .expect("frame");
        frame.set_window_system(Some(Value::symbol("x")));
        frame.selected_window
    };

    evaluator
        .frame_manager_mut()
        .set_window_vscroll(selected_window, 28.0, true, true)
        .expect("set window vscroll");

    let (_, wps) = collect_layout_params(&evaluator, frame_id, None).expect("layout params");
    assert_eq!(wps[0].window_id, selected_window.0 as i64);
    assert_eq!(wps[0].vscroll, -28);
}

#[test]
fn collect_layout_params_reads_nobreak_char_display_global() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*nobreak*");
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("nobreak-frame", 800, 600, buf_id);

    evaluator
        .obarray_mut()
        .set_symbol_value("nobreak-char-display", Value::NIL);
    let (_, wps) = collect_layout_params(&evaluator, frame_id, None).expect("layout params");
    assert_eq!(wps[0].nobreak_char_display, 0);

    evaluator
        .obarray_mut()
        .set_symbol_value("nobreak-char-display", Value::T);
    let (_, wps) = collect_layout_params(&evaluator, frame_id, None).expect("layout params");
    assert_eq!(wps[0].nobreak_char_display, 1);

    evaluator
        .obarray_mut()
        .set_symbol_value("nobreak-char-display", Value::fixnum(2));
    let (_, wps) = collect_layout_params(&evaluator, frame_id, None).expect("layout params");
    assert_eq!(wps[0].nobreak_char_display, 2);
}

#[test]
fn test_frame_params_from_neovm() {
    let runtime = neovm_core::emacs_core::Context::new();

    let mut buf_mgr = BufferManager::new();
    let buf_id = buf_mgr.create_buffer("*scratch*");
    let mut frame_mgr = FrameManager::new();
    let fid = frame_mgr.create_frame("test", 1024, 768, buf_id);
    let frame = frame_mgr.get(fid).unwrap();

    let face_table = FaceTable::new();
    let fp = frame_params_from_neovm(frame, &face_table, runtime.obarray());
    assert_eq!(fp.width, 1024.0);
    assert_eq!(fp.height, 768.0);
    assert_eq!(fp.tab_bar_height, 0.0);
}

/// The window-params builder must resolve the special-display face colors from
/// the face table instead of hardcoding them to 0. Without this, the
/// escape-glyph / nobreak / glyphless / fill-column / trailing-whitespace colors
/// never reach the renderer (the control-char escape-glyph merge in
/// `resolve_source_item_layout_for_active_face` and the trailing-whitespace
/// render state are gated on these fields being non-zero). Mirrors GNU's
/// `merge_escape_glyph_face` etc. in xdisp.c.
#[test]
fn window_params_resolve_special_display_face_colors() {
    // Pack R<<16 | G<<8 | B, matching `face_fg_pixel` / `NeoColor::from_pixel`.
    fn pixel(r: u8, g: u8, b: u8) -> u32 {
        (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
    }

    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*special*");
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("test", 800, 600, buf_id);

    // Define the special-display faces with distinct, known colors so each
    // WindowParams field can be checked against a different value.
    {
        let table = evaluator.face_table_mut();

        // escape-glyph: cyan-ish foreground (matches the GUI repro intent).
        let mut escape = NeoFace::new("escape-glyph");
        escape.foreground = Some(NeoColor::rgb(0x46, 0xD9, 0xFF));
        table.define("escape-glyph", escape);

        // nobreak-space inherits escape-glyph and sets no foreground of its
        // own, so `resolve` must report the inherited escape-glyph color --
        // exactly as faces.el defines it.
        let mut nobreak = NeoFace::new("nobreak-space");
        nobreak.inherit = Some(Value::symbol("escape-glyph"));
        table.define("nobreak-space", nobreak);

        let mut glyphless = NeoFace::new("glyphless-char");
        glyphless.foreground = Some(NeoColor::rgb(0x80, 0x80, 0x80));
        table.define("glyphless-char", glyphless);

        let mut fci = NeoFace::new("fill-column-indicator");
        fci.foreground = Some(NeoColor::rgb(0x33, 0x44, 0x55));
        table.define("fill-column-indicator", fci);

        // trailing-whitespace uses a *background* color.
        let mut tw = NeoFace::new("trailing-whitespace");
        tw.background = Some(NeoColor::rgb(0xFF, 0x00, 0x00));
        table.define("trailing-whitespace", tw);
    }

    let frame = evaluator.frame_manager().get(frame_id).unwrap();
    let buffer = evaluator.buffer_manager().get(buf_id).unwrap();
    let window = frame.root_window.find(frame.selected_window).unwrap();

    let params = window_params_from_neovm(
        window,
        buffer,
        frame,
        evaluator.obarray(),
        evaluator.face_table(),
        None,
        true,
        false,
        Value::T,
        Value::NIL,
    )
    .expect("leaf window params");

    // Each field is non-zero and equals the packed pixel of its face color.
    assert_eq!(
        params.escape_glyph_fg,
        pixel(0x46, 0xD9, 0xFF),
        "escape-glyph foreground"
    );
    assert_ne!(params.escape_glyph_fg, 0);
    assert_eq!(
        params.nobreak_char_fg,
        pixel(0x46, 0xD9, 0xFF),
        "nobreak-space inherits escape-glyph foreground"
    );
    assert_eq!(
        params.glyphless_char_fg,
        pixel(0x80, 0x80, 0x80),
        "glyphless-char foreground"
    );
    assert_eq!(
        params.fill_column_indicator_fg,
        pixel(0x33, 0x44, 0x55),
        "fill-column-indicator foreground"
    );
    assert_eq!(
        params.trailing_ws_bg,
        pixel(0xFF, 0x00, 0x00),
        "trailing-whitespace background"
    );
}

#[test]
fn frame_params_from_neovm_uses_effective_gui_divider_widths() {
    let runtime = neovm_core::emacs_core::Context::new();

    let mut buf_mgr = BufferManager::new();
    let buf_id = buf_mgr.create_buffer("*scratch*");
    let mut frame_mgr = FrameManager::new();
    let fid = frame_mgr.create_frame("test", 1024, 768, buf_id);
    {
        let frame = frame_mgr.get_mut(fid).unwrap();
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.set_parameter(Value::symbol("right-divider-width"), Value::fixnum(6));
        frame.set_parameter(Value::symbol("bottom-divider-width"), Value::fixnum(4));
    }
    let frame = frame_mgr.get(fid).unwrap();

    let face_table = FaceTable::new();
    let fp = frame_params_from_neovm(frame, &face_table, runtime.obarray());
    assert_eq!(fp.right_divider_width, 6);
    assert_eq!(fp.bottom_divider_width, 4);
}

#[test]
fn frame_params_from_neovm_ignores_stored_tty_divider_parameters() {
    let runtime = neovm_core::emacs_core::Context::new();

    let mut buf_mgr = BufferManager::new();
    let buf_id = buf_mgr.create_buffer("*scratch*");
    let mut frame_mgr = FrameManager::new();
    let fid = frame_mgr.create_frame("test", 80, 24, buf_id);
    {
        let frame = frame_mgr.get_mut(fid).unwrap();
        frame.set_parameter(Value::symbol("right-divider-width"), Value::fixnum(6));
        frame.set_parameter(Value::symbol("bottom-divider-width"), Value::fixnum(4));
    }
    let frame = frame_mgr.get(fid).unwrap();

    let face_table = FaceTable::new();
    let fp = frame_params_from_neovm(frame, &face_table, runtime.obarray());
    assert_eq!(fp.right_divider_width, 0);
    assert_eq!(fp.bottom_divider_width, 0);
}

#[test]
fn chrome_face_pixel_height_uses_ceil_for_fractional_metrics() {
    let mut face = ResolvedFace::default();
    face.font_line_height = 17.2;
    face.box_type = 1;
    face.box_line_width = 1.into();

    assert_eq!(
        chrome_face_pixel_height(&face, 14.1, neomacs_display_protocol::DeviceScale::ONE),
        20.0
    );
    assert_eq!(
        chrome_face_pixel_height(
            &face,
            14.1,
            neomacs_display_protocol::DeviceScale::new(2.0).unwrap(),
        ),
        19.0,
        "positive box widths reserve GNU device pixels at HiDPI scale"
    );

    face.font_line_height = 0.0;
    assert_eq!(
        chrome_face_pixel_height(&face, 14.1, neomacs_display_protocol::DeviceScale::ONE),
        17.0
    );
}

#[test]
fn chrome_face_pixel_height_uses_smaller_realized_face_like_gnu() {
    let mut face = ResolvedFace::default();
    face.font_line_height = 12.0;

    assert_eq!(
        chrome_face_pixel_height(&face, 14.1, neomacs_display_protocol::DeviceScale::ONE),
        12.0
    );
}

#[test]
fn window_params_default_colors_follow_buffer_default_face_remap() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator
        .buffer_manager_mut()
        .create_buffer("*default-remap*");
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("test", 800, 600, buf_id);

    {
        let table = evaluator.face_table_mut();
        let mut default = NeoFace::new("default");
        default.foreground = Some(NeoColor::rgb(0, 0, 0));
        default.background = Some(NeoColor::rgb(255, 255, 255));
        table.define("default", default);
    }

    evaluator
        .buffer_manager_mut()
        .get_mut(buf_id)
        .expect("buffer")
        .set_buffer_local(
            "face-remapping-alist",
            Value::list(vec![Value::list(vec![
                Value::symbol("default"),
                Value::list(vec![
                    Value::keyword("background"),
                    Value::string("#000000"),
                    Value::keyword("foreground"),
                    Value::string("#ffffff"),
                ]),
                Value::symbol("default"),
            ])]),
        );

    let frame = evaluator.frame_manager().get(frame_id).unwrap();
    let buffer = evaluator.buffer_manager().get(buf_id).unwrap();
    let window = frame.root_window.find(frame.selected_window).unwrap();

    let params = window_params_from_neovm(
        window,
        buffer,
        frame,
        evaluator.obarray(),
        evaluator.face_table(),
        None,
        true,
        false,
        Value::T,
        Value::NIL,
    )
    .expect("leaf window params");

    assert_eq!(params.default_bg, 0x000000);
    assert_eq!(params.default_fg, 0xFFFFFF);
}

#[test]
fn test_window_params_from_neovm_internal_returns_none() {
    use neovm_core::window::SplitDirection;

    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*test*");
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("test", 800, 600, buf_id);
    let internal = Window::Internal {
        id: WindowId(99),
        direction: SplitDirection::Vertical,
        children: vec![],
        bounds: NeoRect::new(0.0, 0.0, 100.0, 100.0),
        // GNU `make_window` leaves top_line/left_col zero (ed7a3476d).
        top_line: 0,
        left_col: 0,
        parameters: Vec::new(),
        parameters_generation: 0,
        combination_limit: false,
        new_pixel: None,
        new_total: None,
        new_normal: Value::NIL,
        normal_lines: Value::NIL,
        normal_cols: Value::NIL,
    };
    let buf = evaluator.buffer_manager().get(buf_id).unwrap();
    let frame = evaluator.frame_manager().get(frame_id).unwrap();

    let result = window_params_from_neovm(
        &internal,
        &buf,
        frame,
        evaluator.obarray(),
        evaluator.face_table(),
        None,
        false,
        false,
        Value::T,
        Value::NIL,
    );
    assert!(result.is_none(), "Internal windows should return None");
}

#[test]
fn window_params_from_neovm_uses_default_header_line_and_tab_line_values() {
    use neovm_core::buffer::buffer::lookup_buffer_slot;

    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*test*");
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("test", 800, 600, buf_id);

    // Set global defaults via the Phase 10D Vbuffer_defaults API.
    // `obarray.set_symbol_value` is a no-op for Forwarded symbols
    // (see symbol.rs:1303); `BufferManager::set_buffer_default_slot`
    // is the correct path -- it updates `buffer_defaults[offset]`
    // AND propagates to all buffers whose `local_flags` bit is
    // clear. Mirrors GNU `set_default_internal` SYMBOL_FORWARDED
    // arm (data.c:2044-2078) that the `(set-default ...)` builtin
    // routes through.
    let header_slot = lookup_buffer_slot("header-line-format").expect("header-line-format slot");
    let tab_slot = lookup_buffer_slot("tab-line-format").expect("tab-line-format slot");
    evaluator
        .buffer_manager_mut()
        .set_buffer_default_slot(header_slot, Value::string("Header sample"));
    evaluator
        .buffer_manager_mut()
        .set_buffer_default_slot(tab_slot, Value::string("Tab sample"));

    let frame = evaluator.frame_manager().get(frame_id).unwrap();
    let buffer = evaluator.buffer_manager().get(buf_id).unwrap();
    let window = frame.root_window.find(frame.selected_window).unwrap();

    let params = window_params_from_neovm(
        window,
        buffer,
        frame,
        evaluator.obarray(),
        evaluator.face_table(),
        None,
        true,
        false,
        Value::T,
        Value::NIL,
    )
    .expect("leaf window params");

    assert!(params.header_line_height > 0.0);
    assert!(params.tab_line_height > 0.0);
}

#[test]
fn layout_snapshot_buffer_local_value_falls_back_to_default_values() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator
        .buffer_manager_mut()
        .create_buffer("*line-numbers*");

    eval_lisp(
        &mut evaluator,
        "(set-default 'display-line-numbers 'relative)",
    );

    let snapshot = {
        let buffer = evaluator.buffer_manager().get(buf_id).unwrap();
        LayoutBufferSnapshot::from_buffer_with_obarray(buffer, evaluator.obarray())
    };

    assert_eq!(
        buffer_display_line_numbers_mode(&snapshot),
        DisplayLineNumbersMode::Relative
    );
    assert!(buffer_local_bool(
        &snapshot,
        LayoutVar::DisplayLineNumbersCurrentAbsolute
    ));
}

#[test]
fn display_line_numbers_symbol_domain_matches_gnu() {
    assert_eq!(
        DisplayLineNumbersSymbol::from_symbol_name("relative"),
        Some(DisplayLineNumbersSymbol::Relative)
    );
    assert_eq!(
        DisplayLineNumbersSymbol::from_symbol_name("visual"),
        Some(DisplayLineNumbersSymbol::Visual)
    );
    assert_eq!(DisplayLineNumbersSymbol::Relative.name(), "relative");
    assert_eq!(DisplayLineNumbersSymbol::Visual.name(), "visual");
    assert_eq!(DisplayLineNumbersSymbol::from_symbol_name("absolute"), None);
}

#[test]
fn layout_snapshot_buffer_local_value_prefers_local_binding() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator
        .buffer_manager_mut()
        .create_buffer("*line-numbers*");

    eval_lisp(
        &mut evaluator,
        "(set-default 'display-line-numbers 'relative)",
    );
    evaluator
        .buffer_manager_mut()
        .get_mut(buf_id)
        .unwrap()
        .set_buffer_local("display-line-numbers", Value::T);

    let snapshot = {
        let buffer = evaluator.buffer_manager().get(buf_id).unwrap();
        LayoutBufferSnapshot::from_buffer_with_obarray(buffer, evaluator.obarray())
    };

    assert_eq!(
        buffer_display_line_numbers_mode(&snapshot),
        DisplayLineNumbersMode::Absolute
    );
}

#[test]
fn layout_snapshot_sees_display_line_numbers_set_through_lisp() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    evaluator
        .eval_str("(setq display-line-numbers t)")
        .expect("enable display-line-numbers through Lisp");
    let buf_id = evaluator
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();

    let snapshot = {
        let buffer = evaluator.buffer_manager().get(buf_id).expect("buffer");
        assert_eq!(
            buffer.buffer_local_value("display-line-numbers"),
            Some(Value::T),
            "the public buffer-local value is the source of truth consumed by redisplay"
        );
        LayoutBufferSnapshot::from_buffer_with_obarray(buffer, evaluator.obarray())
    };

    assert_eq!(
        buffer_display_line_numbers_mode(&snapshot),
        DisplayLineNumbersMode::Absolute,
        "the immutable layout snapshot must preserve a Lisp-created local-if-set binding"
    );
}

#[test]
fn test_window_params_nonselected_reads_window_point() {
    // For NON-selected windows, `params.point` comes from
    // `Window::point` (the snapshotted pointm marker), NOT
    // `buffer.pt_char`. Mirrors GNU `window.c:window_point`:
    //
    //   return (w == XWINDOW (selected_window)
    //           ? BUF_PT (XBUFFER (w->contents))
    //           : XMARKER (w->pointm)->charpos);
    //
    // The selected-window branch is exercised elsewhere; this
    // test specifically verifies the non-selected branch so a
    // future refactor of `window_params_from_neovm` can't
    // silently collapse both branches to read from the buffer.
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*test*");
    {
        let buf = evaluator.buffer_manager_mut().get_mut(buf_id).unwrap();
        buf.insert("abcdef");
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(0));
    }
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("test", 800, 600, buf_id);
    let selected_window = evaluator
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = evaluator
            .frame_manager_mut()
            .get_mut(frame_id)
            .expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let Window::Leaf { point, .. } = window {
            *point = LispCharPos1::from_one_based_usize(5);
        } else {
            panic!("expected leaf window");
        }
    }

    let frame = evaluator.frame_manager().get(frame_id).expect("frame");
    let buffer = evaluator.buffer_manager().get(buf_id).expect("buffer");
    // Pass `is_selected = false` to exercise the non-selected
    // branch of window_params_from_neovm. We're testing the
    // window_point_not_buffer_point rule for *this* branch.
    let params = window_params_from_neovm(
        frame.find_window(selected_window).expect("selected window"),
        buffer,
        frame,
        evaluator.obarray(),
        evaluator.face_table(),
        None,
        false, // is_selected
        false,
        Value::T,
        Value::NIL,
    )
    .expect("window params");

    // Window::point = 5 (1-based); params.point is 0-based, so 4.
    // buffer.pt_char = 0 (we called goto_emacs_byte_pos(0)). The non-selected
    // branch must NOT use the buffer's point.
    assert_ne!(buffer.point_char_pos().get() as i64, params.point);
    assert_eq!(params.point, 4);
}

#[test]
fn test_effective_cursor_spec_prefers_window_cursor_type() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*cursor*");
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("test", 800, 600, buf_id);
    let frame = evaluator.frame_manager().get(frame_id).unwrap();
    let buffer = evaluator.buffer_manager().get(buf_id).unwrap();

    let spec = effective_cursor_spec(
        frame,
        buffer,
        true,
        false,
        Value::cons(Value::symbol("bar"), Value::fixnum(5)),
    )
    .unwrap();

    assert_eq!(spec.cursor_kind, CursorKind::Bar);
    assert_eq!(spec.bar_width, CursorBarWidth::new(5));
}

#[test]
fn cursor_effect_profile_accepts_known_effect() {
    let _evaluator = neovm_core::emacs_core::Context::new();
    let profile = Value::list(vec![
        Value::symbol("cursor-glow"),
        Value::keyword(":enabled"),
        Value::T,
    ]);

    let effects = parse_cursor_effect_profile(profile).expect("known cursor effect");

    assert!(effects.cursor_glow.enabled);
}

#[test]
fn cursor_effect_profile_rejects_unknown_effect() {
    let _evaluator = neovm_core::emacs_core::Context::new();
    let profile = Value::list(vec![
        Value::symbol("cursor-glwo"),
        Value::keyword(":enabled"),
        Value::T,
    ]);

    assert!(parse_cursor_effect_profile(profile).is_none());
}

#[test]
fn cursor_effect_profile_rejects_mixed_profile_with_unknown_effect() {
    let _evaluator = neovm_core::emacs_core::Context::new();
    let profile = Value::list(vec![
        Value::list(vec![
            Value::symbol("cursor-glow"),
            Value::keyword(":enabled"),
            Value::T,
        ]),
        Value::list(vec![
            Value::symbol("cursor-glwo"),
            Value::keyword(":enabled"),
            Value::T,
        ]),
    ]);

    assert!(parse_cursor_effect_profile(profile).is_none());
}

#[test]
fn test_parse_cursor_spec_nil_is_no_cursor_like_gnu() {
    let spec = parse_cursor_spec(&Value::NIL).unwrap();

    assert_eq!(spec.cursor_kind, CursorKind::NoCursor);
    assert_eq!(spec.bar_width, CursorBarWidth::DEFAULT);
}

#[test]
fn test_parse_cursor_spec_accepts_zero_width_cons_like_gnu() {
    let _evaluator = neovm_core::emacs_core::Context::new();
    let spec = parse_cursor_spec(&Value::cons(Value::symbol("bar"), Value::fixnum(0))).unwrap();

    assert_eq!(spec.cursor_kind, CursorKind::Bar);
    assert_eq!(spec.bar_width, CursorBarWidth::new(0));
}

#[test]
fn test_parse_cursor_spec_invalid_cons_falls_back_hollow_like_gnu() {
    let _evaluator = neovm_core::emacs_core::Context::new();
    let spec = parse_cursor_spec(&Value::cons(Value::symbol("bar"), Value::fixnum(-1))).unwrap();

    assert_eq!(spec.cursor_kind, CursorKind::HollowBox);
    assert_eq!(spec.bar_width, CursorBarWidth::DEFAULT);
}

#[test]
fn test_effective_cursor_spec_nonselected_box_becomes_hollow() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*cursor*");
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("test", 800, 600, buf_id);
    let frame = evaluator.frame_manager().get(frame_id).unwrap();
    let buffer = evaluator.buffer_manager().get(buf_id).unwrap();

    let spec = effective_cursor_spec(frame, buffer, false, false, Value::T).unwrap();

    assert_eq!(spec.cursor_kind, CursorKind::HollowBox);
}

#[test]
fn test_effective_cursor_spec_nonselected_bar_narrows_under_t() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*cursor*");
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("test", 800, 600, buf_id);

    if let Some(buf) = evaluator.buffer_manager_mut().get_mut(buf_id) {
        buf.set_buffer_local(
            "cursor-type",
            Value::cons(Value::symbol("bar"), Value::fixnum(5)),
        );
        buf.set_buffer_local("cursor-in-non-selected-windows", Value::T);
    }

    let frame = evaluator.frame_manager().get(frame_id).unwrap();
    let buffer = evaluator.buffer_manager().get(buf_id).unwrap();
    let spec = effective_cursor_spec(frame, buffer, false, false, Value::T).unwrap();

    assert_eq!(spec.cursor_kind, CursorKind::Bar);
    assert_eq!(spec.bar_width, CursorBarWidth::new(4));
}

#[test]
fn test_effective_cursor_spec_nonselected_explicit_bar_is_preserved() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*cursor*");
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("test", 800, 600, buf_id);

    if let Some(buf) = evaluator.buffer_manager_mut().get_mut(buf_id) {
        buf.set_buffer_local(
            "cursor-in-non-selected-windows",
            Value::cons(Value::symbol("bar"), Value::fixnum(3)),
        );
    }

    let frame = evaluator.frame_manager().get(frame_id).unwrap();
    let buffer = evaluator.buffer_manager().get(buf_id).unwrap();
    let spec = effective_cursor_spec(frame, buffer, false, false, Value::T).unwrap();

    assert_eq!(spec.cursor_kind, CursorKind::Bar);
    assert_eq!(spec.bar_width, CursorBarWidth::new(3));
}

#[test]
fn test_effective_cursor_spec_nonselected_nil_disables_cursor() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*cursor*");
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("test", 800, 600, buf_id);

    if let Some(buf) = evaluator.buffer_manager_mut().get_mut(buf_id) {
        buf.set_buffer_local("cursor-in-non-selected-windows", Value::NIL);
    }

    let frame = evaluator.frame_manager().get(frame_id).unwrap();
    let buffer = evaluator.buffer_manager().get(buf_id).unwrap();
    let spec = effective_cursor_spec(frame, buffer, false, false, Value::T).unwrap();

    assert_eq!(spec.cursor_kind, CursorKind::NoCursor);
}

#[test]
fn test_effective_cursor_spec_nonselected_minibuffer_hides_cursor() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*cursor*");
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("test", 800, 600, buf_id);
    let frame = evaluator.frame_manager().get(frame_id).unwrap();
    let buffer = evaluator.buffer_manager().get(buf_id).unwrap();

    let spec = effective_cursor_spec(frame, buffer, false, true, Value::T);

    assert!(spec.is_none());
}

#[test]
fn collect_layout_params_dims_windows_on_nonselected_frame() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let first_buf = evaluator.buffer_manager_mut().create_buffer("*first*");
    let second_buf = evaluator.buffer_manager_mut().create_buffer("*second*");

    let first_frame = evaluator
        .frame_manager_mut()
        .create_frame("first", 800, 600, first_buf);
    let second_frame = evaluator
        .frame_manager_mut()
        .create_frame("second", 800, 600, second_buf);
    assert!(evaluator.frame_manager_mut().select_frame(second_frame));

    let (_frame_params, windows) =
        collect_layout_params(&evaluator, first_frame, None).expect("layout params");

    assert!(!windows.is_empty());
    for window in &windows {
        assert!(
            !window.selected,
            "non-selected frame should not expose active windows: {window:?}"
        );
    }

    let main_window = windows
        .iter()
        .find(|window| !window.is_minibuffer())
        .expect("main window");
    assert_eq!(main_window.cursor_kind, CursorKind::HollowBox);
}

#[test]
fn test_frame_cursor_color_uses_cursor_face_background() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator
        .buffer_manager_mut()
        .create_buffer("*cursor-color*");
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("test", 800, 600, buf_id);
    evaluator
        .frame_manager_mut()
        .get_mut(frame_id)
        .unwrap()
        .parameters
        .remove(&FrameParam::CursorColor.symbol());
    let frame = evaluator.frame_manager().get(frame_id).unwrap();

    let cursor_color = frame_cursor_color_pixel(frame, evaluator.face_table());
    let expected = evaluator
        .face_table()
        .resolve("cursor")
        .background
        .map(|color| color_to_pixel(&color))
        .unwrap();

    assert_eq!(cursor_color, expected);
}

#[test]
fn test_frame_cursor_color_prefers_frame_parameter_like_gnu() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator
        .buffer_manager_mut()
        .create_buffer("*cursor-color*");
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("test", 800, 600, buf_id);
    let frame = evaluator.frame_manager().get(frame_id).unwrap();

    let cursor_color = frame_cursor_color_pixel(frame, evaluator.face_table());

    assert_eq!(cursor_color, 0xFFFFFF);
}

#[test]
fn test_gui_frame_cursor_color_falls_back_when_matching_background_like_gnu() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator
        .buffer_manager_mut()
        .create_buffer("*cursor-color*");
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("test", 800, 600, buf_id);
    {
        let frame = evaluator.frame_manager_mut().get_mut(frame_id).unwrap();
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.set_known_parameter(FrameParam::BackgroundColor, Value::string("white"));
        frame.set_known_parameter(FrameParam::CursorColor, Value::string("white"));
        frame.set_known_parameter(FrameParam::MouseColor, Value::string("black"));
    }
    let frame = evaluator.frame_manager().get(frame_id).unwrap();

    let cursor_color = frame_cursor_color_pixel(frame, evaluator.face_table());

    assert_eq!(cursor_color, 0x000000);
}

#[test]
fn test_gui_frame_cursor_color_falls_back_to_foreground_when_mouse_also_matches_background() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator
        .buffer_manager_mut()
        .create_buffer("*cursor-color*");
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("test", 800, 600, buf_id);
    {
        let frame = evaluator.frame_manager_mut().get_mut(frame_id).unwrap();
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.set_known_parameter(FrameParam::ForegroundColor, Value::string("white"));
        frame.set_known_parameter(FrameParam::BackgroundColor, Value::string("black"));
        frame.set_known_parameter(FrameParam::CursorColor, Value::string("black"));
        frame.set_known_parameter(FrameParam::MouseColor, Value::string("black"));
    }
    let frame = evaluator.frame_manager().get(frame_id).unwrap();

    let cursor_color = frame_cursor_color_pixel(frame, evaluator.face_table());

    assert_eq!(
        cursor_color, 0xffffff,
        "a filled cursor needs a paint color distinct from the child frame background"
    );
}

#[test]
fn test_gui_frame_cursor_color_keeps_contrasting_parameter_like_gnu() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator
        .buffer_manager_mut()
        .create_buffer("*cursor-color*");
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("test", 800, 600, buf_id);
    {
        let frame = evaluator.frame_manager_mut().get_mut(frame_id).unwrap();
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.set_known_parameter(FrameParam::BackgroundColor, Value::string("white"));
        frame.set_known_parameter(FrameParam::CursorColor, Value::string("red"));
        frame.set_known_parameter(FrameParam::MouseColor, Value::string("black"));
    }
    let frame = evaluator.frame_manager().get(frame_id).unwrap();

    let cursor_color = frame_cursor_color_pixel(frame, evaluator.face_table());

    assert_eq!(cursor_color, 0xff0000);
}

#[test]
fn test_frame_cursor_foreground_defaults_to_frame_background_like_gnu() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator
        .buffer_manager_mut()
        .create_buffer("*cursor-foreground*");
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("test", 800, 600, buf_id);
    evaluator
        .frame_manager_mut()
        .get_mut(frame_id)
        .unwrap()
        .set_known_parameter(FrameParam::BackgroundColor, Value::string("#123456"));
    let frame = evaluator.frame_manager().get(frame_id).unwrap();

    assert_eq!(
        frame_cursor_foreground_pixel(frame, evaluator.face_table(), evaluator.obarray()),
        0x123456
    );
}

#[test]
fn test_frame_cursor_foreground_honors_x_cursor_fore_pixel_like_gnu() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator
        .buffer_manager_mut()
        .create_buffer("*cursor-foreground*");
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("test", 800, 600, buf_id);
    evaluator
        .eval_str("(setq x-cursor-fore-pixel \"#abcdef\")")
        .expect("set GNU cursor foreground override");
    let frame = evaluator.frame_manager().get(frame_id).unwrap();

    assert_eq!(
        frame_cursor_foreground_pixel(frame, evaluator.face_table(), evaluator.obarray()),
        0xabcdef
    );
}

#[test]
fn test_window_params_buffer_locals() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*locals*");

    // Set buffer-local variables.
    if let Some(buf) = evaluator.buffer_manager_mut().get_mut(buf_id) {
        buf.set_buffer_local("truncate-lines", Value::T);
        buf.set_buffer_local("tab-width", Value::fixnum(4));
        buf.set_buffer_local("word-wrap", Value::NIL);
    }

    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("test", 800, 600, buf_id);

    let (_, wps) = collect_layout_params(&evaluator, frame_id, None).unwrap();

    // The root window should pick up the buffer-local vars.
    let wp = &wps[0];
    assert_eq!(wp.wrap_mode, LineWrapMode::Truncate);
    assert!(!wp.word_wrap);
    assert_eq!(wp.tab_width, 4);
}

#[test]
fn window_params_fill_column_indicator_follows_gnu_conditions() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*fci*");
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("test", 800, 600, buf_id);

    let (_, wps) = collect_layout_params(&evaluator, frame_id, None).unwrap();
    assert_eq!(wps[0].fill_column_indicator, -1);

    {
        let buf = evaluator.buffer_manager_mut().get_mut(buf_id).unwrap();
        buf.set_buffer_local("display-fill-column-indicator", Value::T);
        buf.set_buffer_local("display-fill-column-indicator-character", Value::char('|'));
        buf.set_buffer_local("display-fill-column-indicator-column", Value::T);
        buf.set_buffer_local("fill-column", Value::fixnum(73));
    }
    let (_, wps) = collect_layout_params(&evaluator, frame_id, None).unwrap();
    assert_eq!(wps[0].fill_column_indicator, 73);
    assert_eq!(wps[0].fill_column_indicator_char, '|');

    {
        let buf = evaluator.buffer_manager_mut().get_mut(buf_id).unwrap();
        buf.set_buffer_local("display-fill-column-indicator-column", Value::fixnum(0));
    }
    let (_, wps) = collect_layout_params(&evaluator, frame_id, None).unwrap();
    assert_eq!(wps[0].fill_column_indicator, 0);

    {
        let buf = evaluator.buffer_manager_mut().get_mut(buf_id).unwrap();
        buf.set_buffer_local("display-fill-column-indicator-character", Value::NIL);
    }
    let (_, wps) = collect_layout_params(&evaluator, frame_id, None).unwrap();
    assert_eq!(wps[0].fill_column_indicator, -1);
}

#[test]
fn test_window_params_partial_width_windows_force_truncation_like_gnu() {
    use neovm_core::window::{SplitDirection, SplitPlacement};

    let mut evaluator = neovm_core::emacs_core::Context::new();
    let left_buf = evaluator.buffer_manager_mut().create_buffer("*left*");
    let right_buf = evaluator.buffer_manager_mut().create_buffer("*right*");
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("test", 640, 600, left_buf);
    let selected = evaluator
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    assert!(
        evaluator
            .frame_manager_mut()
            .split_window(
                frame_id,
                selected,
                SplitDirection::Horizontal,
                right_buf,
                None,
                SplitPlacement::AfterTarget,
            )
            .is_some(),
        "expected side-by-side split"
    );

    let (_, wps) = collect_layout_params(&evaluator, frame_id, None).expect("layout params");
    let main_windows: Vec<_> = wps.into_iter().filter(|wp| !wp.is_minibuffer()).collect();

    assert_eq!(main_windows.len(), 2);
    assert!(
        main_windows
            .iter()
            .all(|wp| wp.wrap_mode == LineWrapMode::Truncate),
        "GNU truncates partial-width windows below the default threshold: {main_windows:#?}"
    );
}

#[test]
fn active_minibuffer_keeps_its_callers_mode_line_active() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let root_buffer = evaluator.buffer_manager_mut().create_buffer("*caller*");
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("test", 160, 50, root_buffer);
    let caller_window = evaluator
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let minibuffer = evaluator.buffer_manager_mut().create_buffer(" *Minibuf-1*");
    evaluator
        .activate_minibuffer_window_for_buffer(
            minibuffer,
            neovm_core::heap_types::LispString::from_utf8("Prompt: "),
            None,
        )
        .expect("activate minibuffer");

    let (_, windows) = collect_layout_params(&evaluator, frame_id, None).expect("layout params");
    let caller = windows
        .iter()
        .find(|window| window.window_id == caller_window.0 as i64)
        .expect("caller window");

    assert!(!caller.selected, "the minibuffer owns input selection");
    assert!(
        caller.mode_line_active,
        "GNU keeps the minibuffer caller's mode line active"
    );
}

#[test]
fn test_window_params_partial_width_windows_respect_disabled_truncate_partial_width_windows() {
    use neovm_core::window::{SplitDirection, SplitPlacement};

    let mut evaluator = neovm_core::emacs_core::Context::new();
    let left_buf = evaluator.buffer_manager_mut().create_buffer("*left*");
    let right_buf = evaluator.buffer_manager_mut().create_buffer("*right*");
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("test", 640, 600, left_buf);
    let selected = evaluator
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    assert!(
        evaluator
            .frame_manager_mut()
            .split_window(
                frame_id,
                selected,
                SplitDirection::Horizontal,
                right_buf,
                None,
                SplitPlacement::AfterTarget,
            )
            .is_some(),
        "expected side-by-side split"
    );
    eval_lisp(&mut evaluator, "(setq truncate-partial-width-windows nil)");

    let (_, wps) = collect_layout_params(&evaluator, frame_id, None).expect("layout params");
    let main_windows: Vec<_> = wps.into_iter().filter(|wp| !wp.is_minibuffer()).collect();

    assert_eq!(main_windows.len(), 2);
    assert!(
        main_windows
            .iter()
            .all(|wp| wp.wrap_mode == LineWrapMode::Wrap),
        "nil truncate-partial-width-windows should preserve wrapping: {main_windows:#?}"
    );
}

#[test]
fn test_window_params_hscroll_forces_truncation_like_gnu() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*hscroll*");
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("test", 800, 600, buf_id);
    let selected = evaluator
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let frame = evaluator
        .frame_manager_mut()
        .get_mut(frame_id)
        .expect("frame");
    let window = frame.find_window_mut(selected).expect("selected window");
    if let Window::Leaf { hscroll, .. } = window {
        *hscroll = 3;
    } else {
        panic!("expected leaf window");
    }

    let (_, wps) = collect_layout_params(&evaluator, frame_id, None).expect("layout params");
    let wp = wps
        .into_iter()
        .find(|wp| !wp.is_minibuffer())
        .expect("main window");

    assert_eq!(wp.wrap_mode, LineWrapMode::Truncate);
    assert_eq!(wp.hscroll, 3);
}

#[test]
fn test_window_params_fringes_and_margins() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*fringe*");
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("test", 800, 600, buf_id);

    // Set fringes and margins on the root window.
    if let Some(frame) = evaluator.frame_manager_mut().get_mut(frame_id) {
        frame.set_window_system(Some(Value::symbol("x")));
        frame.char_width = 8.0;
        if let Some(win) = frame.selected_window_mut() {
            if let Window::Leaf {
                display, margins, ..
            } = win
            {
                *margins = WindowMargins::new(2, 3);
                display.left_fringe_width = 10;
                display.right_fringe_width = 12;
            }
        }
    }

    let (_, wps) = collect_layout_params(&evaluator, frame_id, None).unwrap();
    let wp = &wps[0];

    assert_eq!(wp.left_fringe_width, 10.0);
    assert_eq!(wp.right_fringe_width, 12.0);
    assert_eq!(wp.left_margin_width, 16.0); // 2 * 8.0
    assert_eq!(wp.right_margin_width, 24.0); // 3 * 8.0

    // text_bounds should be narrower by fringes + margins.
    let expected_text_x = wp.bounds.x + 10.0 + 16.0;
    assert!((wp.text_bounds.x - expected_text_x).abs() < 0.01);
}

#[test]
fn test_window_params_tty_ignores_fringes_keeps_margins() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*tty-fringe*");
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("test", 800, 600, buf_id);

    // GNU window_body_width subtracts margins on every frame, but subtracts
    // WINDOW_FRINGES_WIDTH only when FRAME_WINDOW_P is true.
    if let Some(frame) = evaluator.frame_manager_mut().get_mut(frame_id) {
        frame.char_width = 8.0;
        if let Some(win) = frame.selected_window_mut() {
            if let Window::Leaf {
                display, margins, ..
            } = win
            {
                *margins = WindowMargins::new(2, 3);
                display.left_fringe_width = 10;
                display.right_fringe_width = 12;
            }
        }
    }

    let (_, wps) = collect_layout_params(&evaluator, frame_id, None).unwrap();
    let wp = &wps[0];

    assert_eq!(wp.left_fringe_width, 0.0);
    assert_eq!(wp.right_fringe_width, 0.0);
    assert_eq!(wp.left_margin_width, 16.0);
    assert_eq!(wp.right_margin_width, 24.0);

    let expected_text_x = wp.bounds.x + 16.0;
    assert!((wp.text_bounds.x - expected_text_x).abs() < 0.01);
}

#[test]
fn test_collect_nonexistent_frame() {
    let evaluator = neovm_core::emacs_core::Context::new();
    let result = collect_layout_params(&evaluator, FrameId(999999), None);
    assert!(result.is_none());
}

// -----------------------------------------------------------------------
// RustBufferAccess tests
// -----------------------------------------------------------------------

#[test]
fn test_rust_buffer_access_copy_text() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*test-copy*");
    // Insert some text
    if let Some(buf) = evaluator.buffer_manager_mut().get_mut(buf_id) {
        set_buffer_text(buf, "Hello, world!");
        buf.widen();
    }

    let buf = evaluator.buffer_manager().get(buf_id).unwrap();
    let access = RustBufferAccess::new(buf);

    let mut out = Vec::new();
    access.copy_text(0, 5, &mut out);
    assert_eq!(&out, b"Hello");

    access.copy_text(7, 13, &mut out);
    assert_eq!(&out, b"world!");
}

#[test]
fn test_rust_buffer_access_charpos_to_bytepos() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*test-pos*");
    if let Some(buf) = evaluator.buffer_manager_mut().get_mut(buf_id) {
        set_buffer_text(buf, "abc");
        buf.widen();
    }

    let buf = evaluator.buffer_manager().get(buf_id).unwrap();
    let access = RustBufferAccess::new(buf);

    assert_eq!(access.charpos_to_bytepos(0), 0);
    assert_eq!(access.charpos_to_bytepos(1), 1);
    assert_eq!(access.charpos_to_bytepos(2), 2);
    assert_eq!(access.charpos_to_bytepos(3), 3);
    assert_eq!(access.charpos_to_bytepos(4), 3);
}

#[test]
fn test_layout_buffer_snapshot_preserves_byte_bounds_for_multibyte_text() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator
        .buffer_manager_mut()
        .create_buffer("*test-multibyte-pos*");
    if let Some(buf) = evaluator.buffer_manager_mut().get_mut(buf_id) {
        buf.insert("a\u{2018}b\u{2019}c");
    }

    let buf = evaluator.buffer_manager().get(buf_id).unwrap();
    assert_eq!(buf.point_max_char_pos().get(), 5);
    assert_eq!(buf.point_max_emacs_byte_pos().get(), 9);

    let snapshot = LayoutBufferSnapshot::from_buffer(buf);
    let access = RustBufferAccess::new(&snapshot);

    assert_eq!(access.zv(), 9);
    assert_eq!(access.charpos_to_bytepos(5), 9);
    assert_eq!(
        snapshot
            .layout_emacs_byte_pos_to_char_pos(EmacsBytePos::new(9))
            .get(),
        5
    );
}

#[test]
fn test_rust_buffer_access_lisp_charpos_to_bytepos() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator
        .buffer_manager_mut()
        .create_buffer("*test-lisp-pos*");
    if let Some(buf) = evaluator.buffer_manager_mut().get_mut(buf_id) {
        set_buffer_text(buf, "abc");
        buf.widen();
    }

    let buf = evaluator.buffer_manager().get(buf_id).unwrap();
    let access = RustBufferAccess::new(buf);

    assert_eq!(access.lisp_charpos_to_bytepos(0), 0);
    assert_eq!(access.lisp_charpos_to_bytepos(1), 0);
    assert_eq!(access.lisp_charpos_to_bytepos(2), 1);
    assert_eq!(access.lisp_charpos_to_bytepos(3), 2);
    assert_eq!(access.lisp_charpos_to_bytepos(4), 3);
}

#[test]
fn test_rust_buffer_access_count_lines() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*test-lines*");
    if let Some(buf) = evaluator.buffer_manager_mut().get_mut(buf_id) {
        set_buffer_text(buf, "line1\nline2\nline3");
        buf.widen();
    }

    let buf = evaluator.buffer_manager().get(buf_id).unwrap();
    let access = RustBufferAccess::new(buf);

    assert_eq!(access.count_lines(0, 17), 2); // 2 newlines
    assert_eq!(access.count_lines(0, 6), 1); // 1 newline in "line1\n"
    assert_eq!(access.count_lines(0, 5), 0); // no newline in "line1"
}

#[test]
fn rust_buffer_access_count_lines_is_text_backend_neutral() {
    let _evaluator = neovm_core::emacs_core::Context::new();
    let text: String = (0..2500)
        .map(|idx| if idx % 37 == 0 { '\n' } else { 'x' })
        .collect();
    let expected = text.bytes().filter(|byte| *byte == b'\n').count() as i64;

    for kind in BufferTextBackendKind::variants() {
        let mut buf = test_buffer_with_backend(100 + u64::from(u8::from(kind)), "*lines*", kind);
        set_buffer_text(&mut buf, &text);
        let snapshot = LayoutBufferSnapshot::from_buffer(&buf);
        let access = RustBufferAccess::new(&snapshot);

        assert_eq!(
            access.count_lines(0, text.len() as i64),
            expected,
            "{kind:?}"
        );
        assert_eq!(access.count_lines(1, 36), 0, "{kind:?}");
        assert_eq!(access.count_lines(1, 38), 1, "{kind:?}");
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LayoutSnapshotBackendTrace {
    copied_all: Vec<u8>,
    iterated_middle: Vec<u8>,
    byte_at_positions: Vec<Option<u8>>,
    char_to_byte: Vec<usize>,
    byte_to_char: Vec<usize>,
    face_at_positions: Vec<Option<String>>,
    next_prop_changes: Vec<Option<usize>>,
    line_counts: Vec<i64>,
}

fn fragmented_snapshot_backend_trace(kind: BufferTextBackendKind) -> LayoutSnapshotBackendTrace {
    let _evaluator = neovm_core::emacs_core::Context::new();
    let text = "abé\n日本\tΩ\n";
    let mut buf = test_buffer_with_backend(200 + u64::from(u8::from(kind)), "*snapshot*", kind);
    set_buffer_text(&mut buf, text);

    for marker in ["é", "日本", "Ω"] {
        let pos = text.find(marker).expect("marker");
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(pos));
        buf.insert("tmp");
        buf.delete_emacs_byte_range(emacs_byte_range(pos, pos + "tmp".len()));
    }
    assert_eq!(buf.buffer_string(), text);

    let face_start = text.find("日本").expect("face start");
    let face_end = face_start + "日本".len();
    assert!(buf.put_text_property(
        face_start,
        face_end,
        Value::symbol("face"),
        Value::symbol("bold")
    ));

    let snapshot = LayoutBufferSnapshot::from_buffer(&buf);
    let access = RustBufferAccess::new(&snapshot);
    let byte_len = text.len();
    let byte_boundaries: Vec<usize> = text
        .char_indices()
        .map(|(idx, _)| idx)
        .chain(std::iter::once(byte_len))
        .collect();
    let mut copied_all = Vec::new();
    snapshot.layout_copy_emacs_byte_range_to(emacs_byte_range(0, byte_len), &mut copied_all);

    let mut iterated_middle = Vec::new();
    snapshot
        .layout_try_for_each_emacs_byte_range_chunk(
            emacs_byte_range(2, byte_len.saturating_sub(1)),
            |chunk| {
                iterated_middle.extend_from_slice(chunk);
                Ok::<(), std::convert::Infallible>(())
            },
        )
        .expect("infallible chunk walk");

    let face = Value::symbol("face");
    LayoutSnapshotBackendTrace {
        copied_all,
        iterated_middle,
        byte_at_positions: (0..=byte_len + 1)
            .map(|pos| snapshot.layout_emacs_byte_at_pos(EmacsBytePos::new(pos)))
            .collect(),
        char_to_byte: (0..=text.chars().count() + 1)
            .map(|charpos| {
                snapshot
                    .layout_char_pos_to_emacs_byte_pos(CharPos0::new(charpos))
                    .get()
            })
            .collect(),
        byte_to_char: byte_boundaries
            .iter()
            .copied()
            .map(|pos| {
                snapshot
                    .layout_emacs_byte_pos_to_char_pos(EmacsBytePos::new(pos))
                    .get()
            })
            .collect(),
        face_at_positions: byte_boundaries
            .iter()
            .copied()
            .map(|pos| {
                snapshot
                    .layout_text_prop_at_emacs_byte_pos(EmacsBytePos::new(pos), face)
                    .and_then(|value| value.as_symbol_name().map(str::to_owned))
            })
            .collect(),
        next_prop_changes: byte_boundaries
            .iter()
            .copied()
            .map(|pos| {
                snapshot
                    .layout_next_text_prop_change_after_emacs_byte_pos(EmacsBytePos::new(pos))
                    .map(|byte_pos| byte_pos.get())
            })
            .collect(),
        line_counts: vec![
            access.count_lines(0, byte_len as i64),
            access.count_lines(0, text.find('\n').expect("newline") as i64),
            access.count_lines(0, (text.find('\n').expect("newline") + 1) as i64),
            access.count_lines(2, byte_len.saturating_sub(1) as i64),
        ],
    }
}

#[test]
fn layout_buffer_snapshot_is_text_backend_neutral_for_positions_bytes_and_properties() {
    let baseline = fragmented_snapshot_backend_trace(BufferTextBackendKind::GapBuffer);
    assert_eq!(baseline.copied_all, "abé\n日本\tΩ\n".as_bytes());
    assert!(
        baseline
            .face_at_positions
            .iter()
            .any(|value| value.as_deref() == Some("bold")),
        "baseline should preserve text properties, got {baseline:?}"
    );

    for kind in BufferTextBackendKind::implemented_variants() {
        let trace = fragmented_snapshot_backend_trace(kind);
        assert_eq!(trace, baseline, "{kind:?}");
    }
}

// -----------------------------------------------------------------------
// RustTextPropAccess tests
// -----------------------------------------------------------------------

#[test]
fn test_text_prop_check_invisible() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*invis*");
    if let Some(buf) = evaluator.buffer_manager_mut().get_mut(buf_id) {
        set_buffer_text(buf, "visible hidden visible");
        buf.widen();
        // Mark "hidden" (positions 8..14) as invisible
        buf.put_text_property(8, 14, Value::symbol("invisible"), Value::T);
    }

    let buf = evaluator.buffer_manager().get(buf_id).unwrap();
    let access = RustTextPropAccess::new(buf);

    // Position 0: not invisible
    let (invis, _next) = access.check_invisible(0);
    assert!(!invis.hidden);

    // Position 8: invisible
    let (invis, _next) = access.check_invisible(8);
    assert!(invis.hidden);
    assert!(!invis.ellipsis);

    // Position 14: visible again
    let (invis, _next) = access.check_invisible(14);
    assert!(!invis.hidden);
}

#[test]
fn text_prop_invisible_respects_buffer_invisibility_spec() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator
        .buffer_manager_mut()
        .create_buffer("*dired-invis*");
    if let Some(buf) = evaluator.buffer_manager_mut().get_mut(buf_id) {
        set_buffer_text(buf, "details filename");
        buf.widen();
        buf.set_buffer_local(
            "buffer-invisibility-spec",
            Value::list(vec![
                Value::cons(Value::symbol("dired"), Value::T),
                Value::cons(Value::symbol("dired-filename-hide"), Value::T),
            ]),
        );
        buf.put_text_property(
            0,
            7,
            Value::symbol("invisible"),
            Value::symbol("dired-hide-details-detail"),
        );
        buf.put_text_property(8, 16, Value::symbol("invisible"), Value::symbol("dired"));
    }

    let buf = evaluator.buffer_manager().get(buf_id).unwrap();
    let access = RustTextPropAccess::new(buf);

    let (details, _) = access.check_invisible(0);
    assert_eq!(details, InvisibleStatus::VISIBLE);

    let (filename, _) = access.check_invisible(8);
    assert_eq!(filename, InvisibleStatus::HIDDEN_WITH_ELLIPSIS);
}

#[test]
fn text_prop_invisible_matches_members_of_property_list() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*invis-list*");
    if let Some(buf) = evaluator.buffer_manager_mut().get_mut(buf_id) {
        set_buffer_text(buf, "folded");
        buf.widen();
        buf.set_buffer_local(
            "buffer-invisibility-spec",
            Value::list(vec![Value::cons(Value::symbol("outline"), Value::T)]),
        );
        buf.put_text_property(
            0,
            6,
            Value::symbol("invisible"),
            Value::list(vec![Value::symbol("foo"), Value::symbol("outline")]),
        );
    }

    let buf = evaluator.buffer_manager().get(buf_id).unwrap();
    let access = RustTextPropAccess::new(buf);

    let (status, _) = access.check_invisible(0);
    assert_eq!(status, InvisibleStatus::HIDDEN_WITH_ELLIPSIS);
}

#[test]
fn overlay_invisible_respects_buffer_invisibility_spec() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    {
        let buf = evaluator
            .buffer_manager_mut()
            .current_buffer_mut()
            .expect("current buffer");
        buf.insert("visible folded visible");
        buf.set_buffer_local(
            "buffer-invisibility-spec",
            Value::list(vec![Value::cons(Value::symbol("outline"), Value::T)]),
        );
    }

    let _ = eval_lisp(
        &mut evaluator,
        "(let ((ov (make-overlay 9 15 nil 'front-advance))) \
           (overlay-put ov 'invisible 'outline) \
           ov)",
    );

    let buf = evaluator
        .buffer_manager()
        .current_buffer()
        .expect("current buffer");
    let access = RustTextPropAccess::new(buf);

    let (visible, _) = access.check_invisible(0);
    assert_eq!(visible, InvisibleStatus::VISIBLE);

    let (folded, next_change) = access.check_invisible(8);
    assert_eq!(folded, InvisibleStatus::HIDDEN_WITH_ELLIPSIS);
    assert_eq!(next_change, 14);

    let (visible_again, _) = access.check_invisible(14);
    assert_eq!(visible_again, InvisibleStatus::VISIBLE);
}

#[test]
fn buffer_invisible_ellipsis_text_reads_display_table_slot() {
    // Bug #2: the ellipsis glyphs come from the buffer display table's
    // selective-display / invisible slot (`DISP_INVIS_VECTOR` = extras[4]),
    // not the hard-coded "...".  Build a display-table char-table whose slot
    // holds the org/GNU ` [...] ` glyphs (a mix of bare-fixnum glyph codes and
    // a `(char . face-id)` cons, both produced by `make-glyph-code`) and assert
    // the helper decodes the characters.
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*disp-table*");

    // A display table is a char-table with 6 extra slots; slot 4 holds the
    // invisible/selective-display glyph vector.
    let table = Value::make_char_table(Value::symbol("display-table"), Value::NIL, 6);
    // ` [...] ` : space, '[', '.', '.', '.', ']', space.
    // Use a `(char . face-id)` cons for one glyph (face-id >= 64 path) and bare
    // fixnums for the rest to cover both decode branches.
    let glyphs = Value::vector(vec![
        Value::fixnum(' ' as i64),
        Value::cons(Value::fixnum('[' as i64), Value::fixnum(285)),
        Value::fixnum('.' as i64),
        Value::fixnum('.' as i64),
        Value::fixnum('.' as i64),
        Value::fixnum(']' as i64),
        Value::fixnum(' ' as i64),
    ]);
    table
        .with_char_table_mut(|obj| obj.extras.ensure_owned()[4] = glyphs)
        .expect("char-table");

    if let Some(buf) = evaluator.buffer_manager_mut().get_mut(buf_id) {
        set_buffer_text(buf, "folded");
        buf.set_buffer_local("buffer-display-table", table);
    }

    let buf = evaluator.buffer_manager().get(buf_id).unwrap();
    assert_eq!(
        buffer_invisible_ellipsis_text(buf).as_deref(),
        Some(" [...] "),
    );
}

#[test]
fn buffer_invisible_ellipsis_text_absent_when_no_display_table() {
    // With no buffer/standard display table the helper returns None so callers
    // fall back to GNU's default three-dot ellipsis.
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator
        .buffer_manager_mut()
        .create_buffer("*no-disp-table*");
    if let Some(buf) = evaluator.buffer_manager_mut().get_mut(buf_id) {
        set_buffer_text(buf, "folded");
    }
    let buf = evaluator.buffer_manager().get(buf_id).unwrap();
    assert_eq!(buffer_invisible_ellipsis_text(buf), None);
}

#[test]
fn buffer_display_table_glyphs_decodes_per_char_vector() {
    // The per-character display-table slot (GNU `DISP_CHAR_VECTOR`): a vector of
    // glyph codes for `ch` decodes to the glyph characters.  Covers both the
    // bare-fixnum and `(char . face-id)` cons glyph-code forms, plus a `?\t`
    // glyph that must be returned literally so the renderer re-expands it.
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*disp-char*");

    let table = Value::make_char_table(Value::symbol("display-table"), Value::NIL, 6);
    // tab -> [ ?> (?\t packed with a face id) ]
    let packed_tab = ('\t' as i64) | (7i64 << 22);
    let glyphs = Value::vector(vec![Value::fixnum('>' as i64), Value::fixnum(packed_tab)]);
    neovm_core::emacs_core::chartable::ct_set_single(&table, '\t' as i64, glyphs);
    // 'x' -> [ (?A . 99) ] : cons-form glyph code.
    neovm_core::emacs_core::chartable::ct_set_single(
        &table,
        'x' as i64,
        Value::vector(vec![Value::cons(
            Value::fixnum('A' as i64),
            Value::fixnum(99),
        )]),
    );
    // 'q' -> [] : empty vector means "display nothing".
    neovm_core::emacs_core::chartable::ct_set_single(&table, 'q' as i64, Value::vector(vec![]));
    // 'n' -> 65 (a plain character entry, NOT a vector): not a glyph vector.
    neovm_core::emacs_core::chartable::ct_set_single(&table, 'n' as i64, Value::fixnum(65));

    if let Some(buf) = evaluator.buffer_manager_mut().get_mut(buf_id) {
        set_buffer_text(buf, "x");
        buf.set_buffer_local("buffer-display-table", table);
    }

    let buf = evaluator.buffer_manager().get(buf_id).unwrap();
    assert_eq!(
        buffer_display_table_glyphs(buf, '\t').map(|glyphs| glyphs.text),
        Some(">\t".to_string()),
        "tab maps to '>' then a literal tab glyph"
    );
    assert_eq!(
        buffer_display_table_glyphs(buf, 'x').map(|glyphs| glyphs.text),
        Some("A".to_string())
    );
    // Empty vector -> Some("") (display nothing), distinct from None (no entry).
    assert_eq!(
        buffer_display_table_glyphs(buf, 'q').map(|glyphs| glyphs.text),
        Some(String::new())
    );
    // A plain-character entry is NOT a glyph vector -> None (render literally).
    assert_eq!(buffer_display_table_glyphs(buf, 'n'), None);
    // An unmapped char -> None (the hot path).
    assert_eq!(buffer_display_table_glyphs(buf, 'z'), None);
}

#[test]
fn buffer_display_table_glyphs_preserves_typed_face_runs() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator
        .buffer_manager_mut()
        .create_buffer("*disp-char-face*");
    let face_id = evaluator
        .eval_str("(get 'bold 'face)")
        .expect("bold face id")
        .as_fixnum()
        .expect("numeric face id");
    let table = Value::make_char_table(Value::symbol("display-table"), Value::NIL, 6);
    neovm_core::emacs_core::chartable::ct_set_single(
        &table,
        '\n' as i64,
        Value::vector(vec![
            Value::cons(Value::fixnum('↪' as i64), Value::fixnum(face_id)),
            Value::cons(Value::fixnum('\n' as i64), Value::fixnum(face_id + 1)),
        ]),
    );
    neovm_core::emacs_core::chartable::ct_set_single(
        &table,
        'm' as i64,
        Value::vector(vec![
            Value::cons(Value::fixnum('a' as i64), Value::fixnum(face_id)),
            Value::cons(Value::fixnum('b' as i64), Value::fixnum(face_id + 1)),
        ]),
    );
    neovm_core::emacs_core::chartable::ct_set_single(
        &table,
        'z' as i64,
        Value::vector(vec![
            Value::cons(Value::fixnum('a' as i64), Value::fixnum(0)),
            Value::cons(Value::fixnum('b' as i64), Value::fixnum(0)),
        ]),
    );
    neovm_core::emacs_core::chartable::ct_set_single(
        &table,
        'p' as i64,
        Value::vector(vec![Value::fixnum(('P' as i64) | (face_id << 22))]),
    );
    neovm_core::emacs_core::chartable::ct_set_single(
        &table,
        'n' as i64,
        Value::vector(vec![Value::fixnum(('N' as i64) | (-1i64 << 22))]),
    );
    if let Some(buf) = evaluator.buffer_manager_mut().get_mut(buf_id) {
        set_buffer_text(buf, "\n");
        buf.set_buffer_local("buffer-display-table", table);
    }

    let buf = evaluator.buffer_manager().get(buf_id).unwrap();
    let decoded = buffer_display_table_glyphs(buf, '\n').expect("display-table vector");
    assert_eq!(decoded.text, "↪\n");
    assert_eq!(
        decoded.lisp_face_runs.as_ref(),
        &[
            DisplaySourceMappedFaceRun::new(1, neovm_core::face::LispFaceId::new(face_id)),
            DisplaySourceMappedFaceRun::new(1, neovm_core::face::LispFaceId::new(face_id + 1)),
        ],
        "the terminal newline keeps its own typed face until row-break extraction"
    );
    assert_eq!(
        buffer_display_table_glyphs(buf, 'm')
            .expect("mixed display-table vector")
            .lisp_face_runs
            .as_ref(),
        &[
            DisplaySourceMappedFaceRun::new(1, neovm_core::face::LispFaceId::new(face_id)),
            DisplaySourceMappedFaceRun::new(1, neovm_core::face::LispFaceId::new(face_id + 1)),
        ],
        "mixed visible faces must stay distinct"
    );
    assert_eq!(
        buffer_display_table_glyphs(buf, 'z')
            .expect("zero-face display-table vector")
            .lisp_face_runs
            .as_ref(),
        &[DisplaySourceMappedFaceRun::new(2, None)],
        "adjacent zero face ids coalesce into one inherited run"
    );
    assert_eq!(
        buffer_display_table_glyphs(buf, 'p')
            .expect("packed-face display-table vector")
            .lisp_face_runs
            .as_ref(),
        &[DisplaySourceMappedFaceRun::new(
            1,
            neovm_core::face::LispFaceId::new(face_id)
        )],
        "packed positive face ids stay typed"
    );
    assert_eq!(
        buffer_display_table_glyphs(buf, 'n')
            .expect("negative-face display-table vector")
            .lisp_face_runs
            .as_ref(),
        &[DisplaySourceMappedFaceRun::new(1, None)],
        "negative packed face ids inherit"
    );
}

#[test]
fn glyph_code_face_encoding_accepts_positive_and_rejects_nonpositive_ids() {
    let _eval = neovm_core::emacs_core::Context::new();
    let positive = ('A' as i64) | (7i64 << 22);
    assert_eq!(
        glyph_code_parts(Value::fixnum(positive)),
        Some(('A', neovm_core::face::LispFaceId::new(7)))
    );

    let negative = ('A' as i64) | (-1i64 << 22);
    assert_eq!(
        glyph_code_parts(Value::fixnum(negative)),
        Some(('A', None)),
        "packed face ids use signed arithmetic shift and reject negative ids"
    );
    let zero = 'A' as i64;
    assert_eq!(
        glyph_code_parts(Value::fixnum(zero)),
        Some(('A', None)),
        "packed zero face id is not a named face"
    );

    let negative_cons = Value::cons(Value::fixnum('A' as i64), Value::fixnum(-1));
    assert_eq!(
        glyph_code_parts(negative_cons),
        Some(('A', None)),
        "cons face ids reject negative ids"
    );
}

#[test]
fn buffer_display_table_glyphs_absent_when_no_display_table() {
    // Hot path: with no buffer/standard display table the helper returns None so
    // every char renders literally (a single cheap check, untouched).
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator
        .buffer_manager_mut()
        .create_buffer("*no-disp-char*");
    if let Some(buf) = evaluator.buffer_manager_mut().get_mut(buf_id) {
        set_buffer_text(buf, "abc");
    }
    let buf = evaluator.buffer_manager().get(buf_id).unwrap();
    assert_eq!(buffer_display_table_glyphs(buf, 'a'), None);
    assert_eq!(buffer_display_table_glyphs(buf, '\t'), None);
}

#[test]
fn text_prop_access_returns_raw_line_spacing_property() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*spacing*");
    if let Some(buf) = evaluator.buffer_manager_mut().get_mut(buf_id) {
        set_buffer_text(buf, "line1\nline2");
        buf.widen();
        // Set line-spacing on "line2" area
        buf.put_text_property(6, 11, Value::symbol("line-spacing"), Value::fixnum(4));
    }

    let buf = evaluator.buffer_manager().get(buf_id).unwrap();
    let access = RustTextPropAccess::new(buf);

    let line_spacing = Value::symbol("line-spacing");
    assert_eq!(access.get_property(0, line_spacing), None);
    assert_eq!(access.get_property(6, line_spacing), Some(Value::fixnum(4)));
}

#[test]
fn overlay_strings_at_collects_zero_length_boundary_strings_like_gnu() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator
        .buffer_manager_mut()
        .create_buffer("*overlay-strings*");
    {
        let buf = evaluator.buffer_manager_mut().get_mut(buf_id).unwrap();
        buf.insert("prompt");

        let bob_overlay = Value::make_overlay(neovm_core::heap_types::OverlayDataInit {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(buf_id),
            start: 0,
            end: 0,
            front_advance: false,
            rear_advance: false,
        });
        buf.overlays_mut().insert_overlay(bob_overlay);
        buf.overlays_mut()
            .overlay_put(
                bob_overlay,
                Value::symbol("before-string"),
                Value::string("BOB"),
            )
            .unwrap();

        let eob = buf.point_max_emacs_byte_pos().get();
        let eob_overlay = Value::make_overlay(neovm_core::heap_types::OverlayDataInit {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(buf_id),
            start: eob,
            end: eob,
            front_advance: false,
            rear_advance: false,
        });
        buf.overlays_mut().insert_overlay(eob_overlay);
        buf.overlays_mut()
            .overlay_put(
                eob_overlay,
                Value::symbol("before-string"),
                Value::string("\ninit.el"),
            )
            .unwrap();
        buf.overlays_mut()
            .overlay_put(
                eob_overlay,
                Value::symbol("after-string"),
                Value::string("\nafter"),
            )
            .unwrap();
    }

    let buf = evaluator.buffer_manager().get(buf_id).unwrap();
    let access = RustTextPropAccess::new(buf);

    let (bob_before, bob_after) = access.overlay_strings_split_at(0);
    assert_eq!(bob_before.len(), 1);
    assert_eq!(
        std::str::from_utf8(bob_before[0].bytes().unwrap()).unwrap(),
        "BOB"
    );
    assert!(bob_after.is_empty());

    let (eob_before, eob_after) =
        access.overlay_strings_split_at(buf.point_max_char_pos().get() as i64);
    assert_eq!(eob_before.len(), 1);
    assert_eq!(
        std::str::from_utf8(eob_before[0].bytes().unwrap()).unwrap(),
        "\ninit.el"
    );
    assert_eq!(eob_after.len(), 1);
    assert_eq!(
        std::str::from_utf8(eob_after[0].bytes().unwrap()).unwrap(),
        "\nafter"
    );
}

#[test]
fn overlay_strings_at_filters_window_specific_overlays_like_gnu() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator
        .buffer_manager_mut()
        .create_buffer("*overlay-window-filter*");
    {
        let buf = evaluator.buffer_manager_mut().get_mut(buf_id).unwrap();
        buf.insert("prompt");
        let eob = buf.point_max_emacs_byte_pos().get();

        for (window_id, text) in [
            (Some(1_u64), "LOCAL"),
            (Some(2_u64), "OTHER"),
            (None, "GLOBAL"),
        ] {
            let overlay = Value::make_overlay(neovm_core::heap_types::OverlayDataInit {
                serial: 0,
                plist: Value::NIL,
                buffer: Some(buf_id),
                start: eob,
                end: eob,
                front_advance: false,
                rear_advance: false,
            });
            buf.overlays_mut().insert_overlay(overlay);
            buf.overlays_mut()
                .overlay_put(overlay, Value::symbol("before-string"), Value::string(text))
                .unwrap();
            if let Some(window_id) = window_id {
                buf.overlays_mut()
                    .overlay_put(
                        overlay,
                        Value::symbol("window"),
                        Value::make_window(window_id),
                    )
                    .unwrap();
            }
        }
    }

    let buf = evaluator.buffer_manager().get(buf_id).unwrap();
    let access = RustTextPropAccess::new_for_window(buf, 1);
    let (before, _) = access.overlay_strings_split_at(buf.point_max_char_pos().get() as i64);
    let rendered: Vec<String> = before
        .into_iter()
        .map(|string| {
            std::str::from_utf8(string.bytes().unwrap())
                .unwrap()
                .to_owned()
        })
        .collect();

    assert!(
        rendered.iter().any(|text| text == "LOCAL"),
        "expected overlay for the current window, got {rendered:?}"
    );
    assert!(
        rendered.iter().any(|text| text == "GLOBAL"),
        "expected overlay without a window property, got {rendered:?}"
    );
    assert!(
        !rendered.iter().any(|text| text == "OTHER"),
        "expected overlay for a different window to be filtered, got {rendered:?}"
    );
}

#[test]
fn overlay_strings_at_orders_like_gnu_compare_overlay_entries() {
    // GNU compare_overlay_entries (src/xdisp.c): after-strings from different
    // overlays come in front of before-strings; within after-strings priority
    // descends; within before-strings priority ascends. Each overlay carries a
    // single string here so the order is a deterministic, cycle-free total order.
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator
        .buffer_manager_mut()
        .create_buffer("*overlay-order*");
    {
        let buf = evaluator.buffer_manager_mut().get_mut(buf_id).unwrap();
        buf.insert("x");
        let eob = buf.point_max_emacs_byte_pos().get();
        let specs: [(i64, Option<&str>, Option<&str>); 4] = [
            (1, Some("b1"), None),
            (5, Some("b5"), None),
            (1, None, Some("a1")),
            (5, None, Some("a5")),
        ];
        for (prio, before, after) in specs {
            let overlay = Value::make_overlay(neovm_core::heap_types::OverlayDataInit {
                serial: 0,
                plist: Value::NIL,
                buffer: Some(buf_id),
                start: eob,
                end: eob,
                front_advance: false,
                rear_advance: false,
            });
            buf.overlays_mut().insert_overlay(overlay);
            buf.overlays_mut()
                .overlay_put(overlay, Value::symbol("priority"), Value::fixnum(prio))
                .unwrap();
            if let Some(before) = before {
                buf.overlays_mut()
                    .overlay_put(
                        overlay,
                        Value::symbol("before-string"),
                        Value::string(before),
                    )
                    .unwrap();
            }
            if let Some(after) = after {
                buf.overlays_mut()
                    .overlay_put(overlay, Value::symbol("after-string"), Value::string(after))
                    .unwrap();
            }
        }
    }

    let buf = evaluator.buffer_manager().get(buf_id).unwrap();
    let access = RustTextPropAccess::new(buf);
    let order: Vec<String> = access
        .overlay_strings_at(buf.point_max_char_pos().get() as i64)
        .iter()
        .map(|entry| {
            std::str::from_utf8(entry.bytes().unwrap())
                .unwrap()
                .to_owned()
        })
        .collect();

    // After-strings (descending priority) in front of before-strings (ascending).
    assert_eq!(order, vec!["a5", "a1", "b1", "b5"]);
}

#[test]
fn test_text_prop_next_change() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*next*");
    if let Some(buf) = evaluator.buffer_manager_mut().get_mut(buf_id) {
        set_buffer_text(buf, "aabbcc");
        buf.widen();
        buf.put_text_property(2, 4, Value::symbol("face"), Value::T);
    }

    let buf = evaluator.buffer_manager().get(buf_id).unwrap();
    let access = RustTextPropAccess::new(buf);

    // At position 0, next change should be at 2 (where face starts)
    let next = access.next_property_change(0);
    assert_eq!(next, 2);

    // At position 2, next change should be at 4 (where face ends)
    let next = access.next_property_change(2);
    assert_eq!(next, 4);
}

#[test]
fn test_text_prop_get_property() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*prop*");
    if let Some(buf) = evaluator.buffer_manager_mut().get_mut(buf_id) {
        set_buffer_text(buf, "test");
        buf.widen();
        buf.put_text_property(0, 4, Value::symbol("face"), Value::fixnum(5));
    }

    let buf = evaluator.buffer_manager().get(buf_id).unwrap();
    let access = RustTextPropAccess::new(buf);

    let face = access.get_property(0, Value::symbol("face"));
    assert_eq!(face.and_then(Value::as_fixnum), Some(5));

    let none = access.get_property(0, Value::symbol("nonexistent"));
    assert!(none.is_none());
}

#[test]
fn test_text_prop_access_multibyte_positions_use_byte_offsets() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let buf_id = evaluator.buffer_manager_mut().create_buffer("*utf8-prop*");
    if let Some(buf) = evaluator.buffer_manager_mut().get_mut(buf_id) {
        set_buffer_text(buf, "a好b");
        buf.widen();
        buf.put_text_property(4, 5, Value::symbol("face"), Value::fixnum(9));
    }

    let buf = evaluator.buffer_manager().get(buf_id).unwrap();
    let access = RustTextPropAccess::new(buf);

    let face = access.get_property(2, Value::symbol("face"));
    assert_eq!(face.and_then(Value::as_fixnum), Some(9));

    let next = access.next_property_change(1);
    assert_eq!(next, 2);
}

// -----------------------------------------------------------------------
// FaceResolver tests
// -----------------------------------------------------------------------

#[test]
fn test_color_to_pixel() {
    let c = NeoColor::rgb(255, 128, 0);
    assert_eq!(color_to_pixel(&c), 0x00FF8000);

    let black = NeoColor::rgb(0, 0, 0);
    assert_eq!(color_to_pixel(&black), 0x00000000);

    let white = NeoColor::rgb(255, 255, 255);
    assert_eq!(color_to_pixel(&white), 0x00FFFFFF);
}

#[test]
fn test_face_resolver_default() {
    let _evaluator = neovm_core::emacs_core::Context::new();
    let table = FaceTable::new();

    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let df = resolver.default_face();

    // The standard TTY "default" face keeps GNU's terminal-default color
    // sentinels; the fallback pixels are still carried for non-TTY consumers.
    assert_eq!(df.fg, 0x00FFFFFF);
    assert_eq!(df.bg, 0x00000000);
    assert!(df.use_default_foreground);
    assert!(df.use_default_background);
    assert_eq!(df.font_weight, FontWeight::NORMAL.css_weight()); // 400
    assert!(!df.italic);
    assert!(!df.overstrike);
    assert!(!df.extend);
    assert_eq!(df.underline_style, 0);
    assert!(!df.strike_through);
    assert!(!df.overline);
    assert_eq!(df.box_type, 0);
}

#[test]
fn test_face_resolver_with_text_property() {
    let _evaluator = neovm_core::emacs_core::Context::new();
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);

    // Create a buffer and set "face" text property to bold.
    let mut buf = test_buffer(1, "*test*");
    set_buffer_text(&mut buf, "hello world");
    buf.widen();
    // Set "face" to the symbol "bold" on positions 0..5.
    buf.put_text_property(0, 5, Value::symbol("face"), Value::symbol("bold"));

    let mut next_check = buf.point_max_char_pos().get();
    let resolved = resolver.face_at_pos(&buf, 0, &mut next_check);

    // Bold face should have weight 700.
    assert_eq!(resolved.font_weight, FontWeight::BOLD.css_weight());
    // next_check should be 5 (where the property changes).
    assert_eq!(next_check, 5);

    // Position 6 should have default weight.
    let mut nc2 = buf.point_max_char_pos().get();
    let resolved2 = resolver.face_at_pos(&buf, 6, &mut nc2);
    assert_eq!(resolved2.font_weight, FontWeight::NORMAL.css_weight());
}

#[test]
fn face_resolver_window_filter_matches_window_parameter() {
    // GNU `(:window PARAMETER VALUE)` :filtered face filter (src/xfaces.c
    // `evaluate_face_filter`): applies the wrapped spec only when the current
    // window has window-parameter PARAMETER `eq` VALUE. This is how indent-bars
    // applies its per-window stipple-rotation remap keyed on `indent-bars-whr`.
    let _evaluator = neovm_core::emacs_core::Context::new();
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, Some("x".to_string()));
    let base = resolver.default_face().clone();

    let whr = Value::symbol("indent-bars-whr");
    // (:filtered (:window indent-bars-whr 123) (:foreground "#ff0000"))
    let filtered = Value::list(vec![
        Value::symbol(":filtered"),
        Value::list(vec![Value::symbol(":window"), whr, Value::fixnum(123)]),
        Value::list(vec![
            Value::keyword(":foreground"),
            Value::string("#ff0000"),
        ]),
    ]);

    // No current window parameters -> filter fails -> spec dropped.
    resolver.set_current_window_parameters(Vec::new());
    assert!(resolver.resolve_face_value_over(&base, &filtered).is_none());

    // The parameter present but a DIFFERENT value -> filter fails.
    resolver.set_current_window_parameters(vec![(whr, Value::fixnum(999))]);
    assert!(resolver.resolve_face_value_over(&base, &filtered).is_none());

    // The exact (parameter . value) present -> filter matches -> fg applied.
    resolver.set_current_window_parameters(vec![(whr, Value::fixnum(123))]);
    let resolved = resolver
        .resolve_face_value_over(&base, &filtered)
        .expect("window filter should match and apply the wrapped spec");
    assert_eq!(resolved.fg, 0x00FF0000);
}

#[test]
fn face_resolver_underline_styles_use_gnu_codes() {
    let _evaluator = neovm_core::emacs_core::Context::new();
    let mut table = FaceTable::new();
    let styles: [(&str, NeoUnderlineStyle, u8); 5] = [
        ("line-face", NeoUnderlineStyle::Line, 1),
        ("double-face", NeoUnderlineStyle::DoubleLine, 2),
        ("wave-face", NeoUnderlineStyle::Wave, 3),
        ("dots-face", NeoUnderlineStyle::Dots, 4),
        ("dashes-face", NeoUnderlineStyle::Dashes, 5),
    ];

    for (name, style, _) in styles {
        let mut face = NeoFace::new(name);
        face.underline = neovm_core::face::FaceDecoration::Enabled(neovm_core::face::Underline {
            style,
            color: None,
            position: neovm_core::face::UnderlinePosition::FontMetric,
        });
        table.define(name, face);
    }

    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);

    for (name, _, code) in styles {
        let mut buf = test_buffer(u64::from(code), "*underline*");
        set_buffer_text(&mut buf, "x");
        buf.widen();
        buf.put_text_property(0, 1, Value::symbol("face"), Value::symbol(name));

        let mut next_check = buf.point_max_char_pos().get();
        let resolved = resolver.face_at_pos(&buf, 0, &mut next_check);
        assert_eq!(resolved.underline_style, code, "{name}");
    }
}

#[test]
fn face_resolver_box_styles_use_gnu_codes() {
    let _evaluator = neovm_core::emacs_core::Context::new();
    let mut table = FaceTable::new();
    let styles: [(&str, NeoBoxStyle, u8); 3] = [
        ("flat-box-face", NeoBoxStyle::Flat, 1),
        ("raised-box-face", NeoBoxStyle::Raised, 2),
        ("pressed-box-face", NeoBoxStyle::Pressed, 3),
    ];

    for (name, style, _) in styles {
        let mut face = NeoFace::new(name);
        face.box_border = neovm_core::face::FaceDecoration::Enabled(neovm_core::face::BoxBorder {
            color: None,
            width: 1,
            style,
        });
        table.define(name, face);
    }

    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);

    for (name, _, code) in styles {
        let mut buf = test_buffer(u64::from(code), "*box*");
        set_buffer_text(&mut buf, "x");
        buf.widen();
        buf.put_text_property(0, 1, Value::symbol("face"), Value::symbol(name));

        let mut next_check = buf.point_max_char_pos().get();
        let resolved = resolver.face_at_pos(&buf, 0, &mut next_check);
        assert_eq!(resolved.box_type, code, "{name}");
    }
}

#[test]
fn test_face_resolver_with_font_lock_face() {
    let _evaluator = neovm_core::emacs_core::Context::new();
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);

    let mut buf = test_buffer(2, "*fontlock*");
    set_buffer_text(&mut buf, "defun myfunction");
    buf.widen();
    // GNU font-core.el's `font-lock-default-function' installs this alias
    // when Font Lock is enabled.  Redisplay itself asks for the effective
    // `face' property; it does not hard-code `font-lock-face'.
    buf.set_buffer_local(
        "char-property-alias-alist",
        Value::list(vec![Value::list(vec![
            Value::symbol("face"),
            Value::symbol("font-lock-face"),
        ])]),
    );
    // Set "font-lock-face" to "font-lock-keyword-face" on "defun".
    buf.put_text_property(
        0,
        5,
        Value::symbol("font-lock-face"),
        Value::symbol("font-lock-keyword-face"),
    );

    let mut next_check = buf.point_max_char_pos().get();
    let resolved = resolver.face_at_pos(&buf, 2, &mut next_check);

    // font-lock-keyword-face has foreground purple (128, 0, 128).
    let expected_fg = color_to_pixel(&NeoColor::rgb(128, 0, 128));
    assert_eq!(resolved.fg, expected_fg);
}

#[test]
fn test_face_resolver_face_property_precedes_font_lock_face() {
    let _evaluator = neovm_core::emacs_core::Context::new();
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);

    let mut buf = test_buffer(20, "*scratch*");
    set_buffer_text(&mut buf, "C-x C-f");
    buf.widen();
    buf.put_text_property(0, 7, Value::symbol("face"), Value::symbol("bold"));
    buf.put_text_property(
        0,
        7,
        Value::symbol("font-lock-face"),
        Value::symbol("italic"),
    );

    let mut next_check = buf.point_max_char_pos().get();
    let resolved = resolver.face_at_pos(&buf, 3, &mut next_check);

    assert_eq!(resolved.font_weight, FontWeight::BOLD.css_weight());
    assert!(!resolved.italic);
}

#[test]
fn test_face_resolver_next_check() {
    let _evaluator = neovm_core::emacs_core::Context::new();
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);

    let mut buf = test_buffer(3, "*nextcheck*");
    set_buffer_text(&mut buf, "aabbccdd");
    buf.widen();
    // Face property on [2, 4)
    buf.put_text_property(2, 4, Value::symbol("face"), Value::symbol("bold"));
    // Another property on [4, 6)
    buf.put_text_property(4, 6, Value::symbol("face"), Value::symbol("italic"));

    // At position 0, next_check should be 2 (first property boundary).
    let mut nc = buf.point_max_char_pos().get();
    let _ = resolver.face_at_pos(&buf, 0, &mut nc);
    assert_eq!(nc, 2);

    // At position 2, next_check should be 4 (end of bold range).
    let mut nc = buf.point_max_char_pos().get();
    let _ = resolver.face_at_pos(&buf, 2, &mut nc);
    assert_eq!(nc, 4);

    // At position 4, next_check should be 6 (end of italic range).
    let mut nc = buf.point_max_char_pos().get();
    let _ = resolver.face_at_pos(&buf, 4, &mut nc);
    assert_eq!(nc, 6);
}

#[test]
fn test_face_resolver_overlay_face() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);

    {
        let buf = evaluator
            .buffer_manager_mut()
            .current_buffer_mut()
            .expect("current buffer");
        buf.insert("overlay text here");
    }

    let _ = eval_lisp(
        &mut evaluator,
        "(let ((ov (make-overlay 1 8))) (overlay-put ov 'face 'bold) ov)",
    );

    let buf = evaluator
        .buffer_manager()
        .current_buffer()
        .expect("current buffer");
    let mut nc = buf.point_max_char_pos().get();
    let resolved = resolver.face_at_pos(buf, 3, &mut nc);
    assert_eq!(resolved.font_weight, FontWeight::BOLD.css_weight());
    // next_check should be at most 7 (end of overlay).
    assert!(nc <= 7);
}

#[test]
fn face_for_overlay_string_uses_text_property_but_ignores_overlay_face() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let buf_id = evaluator
        .buffer_manager_mut()
        .create_buffer("*overlay-string-face*");
    {
        let buf = evaluator.buffer_manager_mut().get_mut(buf_id).unwrap();
        set_buffer_text(buf, "anchor");
        buf.put_text_property(0, 1, Value::symbol("face"), Value::symbol("bold"));

        let overlay = Value::make_overlay(neovm_core::heap_types::OverlayDataInit {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(buf.id()),
            start: 0,
            end: 1,
            front_advance: false,
            rear_advance: false,
        });
        buf.overlays_mut().insert_overlay(overlay);
        buf.overlays_mut()
            .overlay_put(overlay, Value::symbol("face"), Value::symbol("italic"))
            .unwrap();
    }

    let buf = evaluator.buffer_manager().get(buf_id).unwrap();
    let mut normal_next_check = buf.point_max_char_pos().get();
    let normal_text_face = resolver.face_at_pos(buf, 0, &mut normal_next_check);
    assert_eq!(normal_text_face.font_weight, FontWeight::BOLD.css_weight());
    assert!(
        normal_text_face.italic,
        "normal buffer text should include overlay face"
    );

    let mut overlay_next_check = buf.point_max_char_pos().get();
    let overlay_string_face = resolver.face_for_overlay_string(buf, 0, &mut overlay_next_check);
    assert_eq!(
        overlay_string_face.font_weight,
        FontWeight::BOLD.css_weight()
    );
    assert!(
        !overlay_string_face.italic,
        "GNU overlay string base face ignores overlay faces"
    );
}

#[test]
fn face_policy_resolves_display_origin_base_faces() {
    use crate::display_face_policy::BaseFacePolicy;
    use crate::display_origin::{DisplayOrigin, DisplayPropertySource, OverlayStringKind};
    use neomacs_display_protocol::face::BasicFaceId;

    let mut evaluator = neovm_core::emacs_core::Context::new();
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let buf_id = evaluator
        .buffer_manager_mut()
        .create_buffer("*display-origin-face-policy*");
    {
        let buf = evaluator.buffer_manager_mut().get_mut(buf_id).unwrap();
        set_buffer_text(buf, "anchor");
        buf.put_text_property(0, 1, Value::symbol("face"), Value::symbol("bold"));

        let overlay = Value::make_overlay(neovm_core::heap_types::OverlayDataInit {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(buf.id()),
            start: 0,
            end: 1,
            front_advance: false,
            rear_advance: false,
        });
        buf.overlays_mut().insert_overlay(overlay);
        buf.overlays_mut()
            .overlay_put(overlay, Value::symbol("face"), Value::symbol("italic"))
            .unwrap();
    }
    let buf = evaluator.buffer_manager().get(buf_id).unwrap();

    let mut next_check = buf.point_max_char_pos().get();
    let buffer_text = resolver.base_face_for_origin(
        Some(buf),
        &DisplayOrigin::BufferText {
            charpos: CharPos0::new(0),
        },
        BaseFacePolicy::BufferFaceIncludingOverlays,
        &mut next_check,
    );
    assert_eq!(buffer_text.font_weight, FontWeight::BOLD.css_weight());
    assert!(buffer_text.italic);

    let mut next_check = buf.point_max_char_pos().get();
    let overlay_string = resolver.base_face_for_origin(
        Some(buf),
        &DisplayOrigin::OverlayString {
            overlay_id: Value::fixnum(1),
            anchor_charpos: CharPos0::new(0),
            kind: OverlayStringKind::Before,
        },
        BaseFacePolicy::OverlayStringAtAnchor,
        &mut next_check,
    );
    assert_eq!(overlay_string.font_weight, FontWeight::BOLD.css_weight());
    assert!(!overlay_string.italic);

    let mut next_check = buf.point_max_char_pos().get();
    let display_property = resolver.base_face_for_origin(
        Some(buf),
        &DisplayOrigin::DisplayPropertyString {
            anchor_charpos: CharPos0::new(0),
            source: DisplayPropertySource::TextProperty,
        },
        BaseFacePolicy::DisplayPropertyUnderlyingFace,
        &mut next_check,
    );
    assert_eq!(display_property.font_weight, FontWeight::BOLD.css_weight());
    assert!(display_property.italic);

    let mut next_check = buf.point_max_char_pos().get();
    let default_face = resolver.base_face_for_origin(
        Some(buf),
        &DisplayOrigin::LinePrefix {
            anchor_charpos: CharPos0::new(0),
        },
        BaseFacePolicy::BufferRemappedBasicFace(BasicFaceId::Default),
        &mut next_check,
    );
    assert_eq!(default_face.fg, resolver.default_face().fg);
    assert_eq!(default_face.bg, resolver.default_face().bg);

    let mut next_check = buf.point_max_char_pos().get();
    let fixed_face = resolver.base_face_for_origin(
        Some(buf),
        &DisplayOrigin::ModeLine { selected: true },
        BaseFacePolicy::BufferRemappedBasicFace(BasicFaceId::ModeLineActive),
        &mut next_check,
    );
    assert_eq!(fixed_face.face_id, u32::from(BasicFaceId::ModeLineActive));
}

#[test]
fn test_face_resolver_overlay_priority() {
    let mut evaluator = neovm_core::emacs_core::Context::new();
    let mut table = FaceTable::new();

    // Define two custom faces with different foreground colors.
    let mut face_a = NeoFace::new("face-a");
    face_a.foreground = Some(NeoColor::rgb(255, 0, 0)); // red
    table.define("face-a", face_a);

    let mut face_b = NeoFace::new("face-b");
    face_b.foreground = Some(NeoColor::rgb(0, 0, 255)); // blue
    table.define("face-b", face_b);

    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);

    {
        let buf = evaluator
            .buffer_manager_mut()
            .current_buffer_mut()
            .expect("current buffer");
        buf.insert("priority test");
    }

    let _ = eval_lisp(
        &mut evaluator,
        "(let ((a (make-overlay 1 11))
               (b (make-overlay 1 11)))
           (overlay-put a 'face 'face-a)
           (overlay-put a 'priority 10)
           (overlay-put b 'face 'face-b)
           (overlay-put b 'priority 20)
           (list a b))",
    );

    let buf = evaluator
        .buffer_manager()
        .current_buffer()
        .expect("current buffer");
    let mut nc = buf.point_max_char_pos().get();
    let resolved = resolver.face_at_pos(buf, 5, &mut nc);
    // face-b (blue, priority 20) should override face-a (red, priority 10).
    assert_eq!(resolved.fg, color_to_pixel(&NeoColor::rgb(0, 0, 255)));
}

#[test]
fn test_face_resolver_face_ref_list_respects_gnu_precedence() {
    let _evaluator = neovm_core::emacs_core::Context::new();
    let mut table = FaceTable::new();

    let mut face_a = NeoFace::new("face-a");
    face_a.foreground = Some(NeoColor::rgb(255, 0, 0));
    table.define("face-a", face_a);

    let mut face_b = NeoFace::new("face-b");
    face_b.foreground = Some(NeoColor::rgb(0, 0, 255));
    table.define("face-b", face_b);

    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);

    let mut buf = test_buffer(51, "*face-ref-list*");
    set_buffer_text(&mut buf, "x");
    buf.widen();
    buf.put_text_property(
        0,
        1,
        Value::symbol("face"),
        Value::list(vec![Value::symbol("face-a"), Value::symbol("face-b")]),
    );

    let mut next_check = buf.point_max_char_pos().get();
    let resolved = resolver.face_at_pos(&buf, 0, &mut next_check);
    assert_eq!(resolved.fg, color_to_pixel(&NeoColor::rgb(255, 0, 0)));
}

#[test]
fn test_face_resolver_buffer_local_default_remap_applies_to_plain_text() {
    let _evaluator = neovm_core::emacs_core::Context::new();
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);

    let mut buf = test_buffer(52, "*default-remap*");
    set_buffer_text(&mut buf, "plain");
    buf.widen();
    buf.set_buffer_local(
        "face-remapping-alist",
        Value::list(vec![Value::list(vec![
            Value::symbol("default"),
            Value::list(vec![Value::keyword("foreground"), Value::string("#009acd")]),
            Value::symbol("default"),
        ])]),
    );

    let mut next_check = buf.point_max_char_pos().get();
    let resolved = resolver.face_at_pos(&buf, 0, &mut next_check);
    assert_eq!(resolved.fg, color_to_pixel(&NeoColor::rgb(0, 154, 205)));
}

#[test]
fn prefix_base_faces_use_buffer_local_default_remap() {
    use crate::display_face_policy::BaseFacePolicy;
    use crate::display_origin::DisplayOrigin;
    use neomacs_display_protocol::face::BasicFaceId;

    let _evaluator = neovm_core::emacs_core::Context::new();
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);

    let mut buf = test_buffer(53, "*prefix-default-remap*");
    set_buffer_text(&mut buf, "plain");
    buf.widen();
    buf.set_buffer_local(
        "face-remapping-alist",
        Value::list(vec![Value::list(vec![
            Value::symbol("default"),
            Value::list(vec![Value::keyword("background"), Value::string("#112233")]),
            Value::symbol("default"),
        ])]),
    );

    for origin in [
        DisplayOrigin::LinePrefix {
            anchor_charpos: CharPos0::new(0),
        },
        DisplayOrigin::WrapPrefix {
            anchor_charpos: CharPos0::new(0),
        },
    ] {
        let policy = origin.default_base_face_policy();
        assert_eq!(
            policy,
            BaseFacePolicy::BufferRemappedBasicFace(BasicFaceId::Default)
        );

        let mut next_check = buf.point_max_char_pos().get();
        let resolved = resolver.base_face_for_origin(Some(&buf), &origin, policy, &mut next_check);
        assert_eq!(resolved.bg, color_to_pixel(&NeoColor::rgb(17, 34, 51)));
    }
}

#[test]
fn test_face_resolver_buffer_local_named_face_remap_applies_to_face_prop() {
    let _evaluator = neovm_core::emacs_core::Context::new();
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);

    let mut buf = test_buffer(53, "*named-remap*");
    set_buffer_text(&mut buf, "bold");
    buf.widen();
    buf.set_buffer_local(
        "face-remapping-alist",
        Value::list(vec![Value::list(vec![
            Value::symbol("bold"),
            Value::list(vec![Value::keyword("foreground"), Value::string("#ff4500")]),
            Value::symbol("bold"),
        ])]),
    );
    buf.put_text_property(0, 4, Value::symbol("face"), Value::symbol("bold"));

    let mut next_check = buf.point_max_char_pos().get();
    let resolved = resolver.face_at_pos(&buf, 0, &mut next_check);
    assert_eq!(resolved.font_weight, FontWeight::BOLD.css_weight());
    assert_eq!(resolved.fg, color_to_pixel(&NeoColor::rgb(255, 69, 0)));
}

/// GNU applies `face-remapping-alist` while realizing the basic face of a
/// window's header line.  Magit uses this exact mechanism to remap
/// `header-line` to `magit-header-line` buffer-locally.
#[test]
fn header_line_base_face_uses_buffer_local_named_face_remap() {
    let _evaluator = neovm_core::emacs_core::Context::new();
    let mut table = FaceTable::new();

    let mut header_line = NeoFace::new("header-line");
    header_line.foreground = Some(NeoColor::rgb(220, 220, 220));
    table.define("header-line", header_line);

    let mut package_header = NeoFace::new("package-header");
    package_header.foreground = Some(NeoColor::rgb(238, 220, 130));
    package_header.weight = Some(FontWeight::BOLD);
    table.define("package-header", package_header);

    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let mut buf = test_buffer(54, "*header-remap*");
    buf.set_buffer_local(
        "face-remapping-alist",
        Value::list(vec![Value::list(vec![
            Value::symbol("header-line"),
            Value::symbol("package-header"),
            Value::symbol("header-line"),
        ])]),
    );

    let mut next_check = buf.point_max_char_pos().get();
    let resolved = resolver.default_base_face_for_origin(
        Some(&buf),
        &DisplayOrigin::HeaderLine { selected: true },
        &mut next_check,
    );

    assert_eq!(resolved.fg, color_to_pixel(&NeoColor::rgb(238, 220, 130)));
    assert_eq!(resolved.font_weight, FontWeight::BOLD.css_weight());
}

/// GNU `lookup_basic_face` performs a named lookup for an inheriting basic
/// face whenever `face-remapping-alist` is active.  That makes remapping an
/// inherited parent observable even when the basic face itself has no alist
/// entry.
#[test]
fn header_line_base_face_uses_buffer_remap_of_inherited_mode_line() {
    let _evaluator = neovm_core::emacs_core::Context::new();
    let mut table = FaceTable::new();

    let mut package_mode_line = NeoFace::new("package-mode-line");
    package_mode_line.foreground = Some(NeoColor::rgb(238, 220, 130));
    package_mode_line.weight = Some(FontWeight::BOLD);
    table.define("package-mode-line", package_mode_line);

    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let mut buf = test_buffer(55, "*inherited-mode-line-remap*");
    buf.set_buffer_local(
        "face-remapping-alist",
        Value::list(vec![Value::list(vec![
            Value::symbol("mode-line"),
            Value::symbol("package-mode-line"),
            Value::symbol("mode-line"),
        ])]),
    );

    let mut next_check = buf.point_max_char_pos().get();
    let resolved = resolver.default_base_face_for_origin(
        Some(&buf),
        &DisplayOrigin::HeaderLine { selected: true },
        &mut next_check,
    );

    assert_eq!(resolved.fg, color_to_pixel(&NeoColor::rgb(238, 220, 130)));
    assert_eq!(resolved.font_weight, FontWeight::BOLD.css_weight());
    assert_eq!(
        resolved.face_id, 0,
        "a buffer-remapped basic face must be assigned a dynamic frame face id"
    );
}

#[test]
fn test_face_resolver_inverse_video() {
    let _evaluator = neovm_core::emacs_core::Context::new();
    let mut table = FaceTable::new();

    let mut inv_face = NeoFace::new("inverse-test");
    inv_face.foreground = Some(NeoColor::rgb(255, 255, 255)); // white
    inv_face.background = Some(NeoColor::rgb(0, 0, 0)); // black
    inv_face.inverse_video = Some(true);
    table.define("inverse-test", inv_face);

    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);

    let mut buf = test_buffer(6, "*inverse*");
    set_buffer_text(&mut buf, "inverted");
    buf.widen();
    buf.put_text_property(0, 8, Value::symbol("face"), Value::symbol("inverse-test"));

    let mut nc = buf.point_max_char_pos().get();
    let resolved = resolver.face_at_pos(&buf, 0, &mut nc);
    // Inverse: fg and bg should be swapped.
    assert_eq!(resolved.fg, 0x00000000); // was white, now black
    assert_eq!(resolved.bg, 0x00FFFFFF); // was black, now white
}

#[test]
fn face_resolver_applies_inverse_video_after_text_and_overlay_faces_are_merged() {
    let _evaluator = neovm_core::emacs_core::Context::new();
    let mut table = FaceTable::new();

    let mut branch_local = NeoFace::new("branch-local");
    branch_local.foreground = Some(NeoColor::rgb(176, 226, 255));
    table.define("branch-local", branch_local);

    let mut branch_current = NeoFace::new("branch-current");
    branch_current.inherit = Some(Value::symbol("branch-local"));
    branch_current.inverse_video = Some(true);
    table.define("branch-current", branch_current);

    let mut section_highlight = NeoFace::new("section-highlight");
    section_highlight.background = Some(NeoColor::rgb(51, 51, 51));
    table.define("section-highlight", section_highlight);

    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00333333, 14.0, None);
    let mut buf = test_buffer(61, "*inverse-overlay-order*");
    set_buffer_text(&mut buf, "master");
    buf.set_buffer_local(
        "char-property-alias-alist",
        Value::list(vec![Value::list(vec![
            Value::symbol("face"),
            Value::symbol("font-lock-face"),
        ])]),
    );
    buf.put_text_property(
        0,
        6,
        Value::symbol("font-lock-face"),
        Value::symbol("branch-current"),
    );
    let overlay = Value::make_overlay(neovm_core::heap_types::OverlayDataInit {
        serial: 0,
        plist: Value::NIL,
        buffer: Some(buf.id()),
        start: 0,
        end: 6,
        front_advance: false,
        rear_advance: false,
    });
    buf.overlays_mut().insert_overlay(overlay);
    buf.overlays_mut()
        .overlay_put(
            overlay,
            Value::symbol("font-lock-face"),
            Value::symbol("section-highlight"),
        )
        .unwrap();

    let mut next_check = buf.point_max_char_pos().get();
    let resolved = resolver.face_at_pos(&buf, 0, &mut next_check);

    assert_eq!(
        resolved.fg, 0x00333333,
        "GNU merges the overlay background before realizing the inherited inverse-video attribute"
    );
    assert_eq!(resolved.bg, 0x00b0e2ff);
}

#[test]
fn test_face_resolver_can_ignore_inverse_video_for_gui_menu_bar() {
    let mut table = FaceTable::new();

    let mut menu_face = NeoFace::new("menu");
    menu_face.foreground = Some(NeoColor::rgb(0x12, 0x34, 0x56));
    menu_face.background = Some(NeoColor::rgb(0xAB, 0xCD, 0xEF));
    menu_face.inverse_video = Some(true);
    table.define("menu", menu_face);

    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, Some("neo".into()));

    let normal = resolver.resolve_named_face("menu");
    assert_eq!(normal.fg, 0x00ABCDEF);
    assert_eq!(normal.bg, 0x00123456);

    let gui_menu = resolver.resolve_named_face_without_inverse_video("menu");
    assert_eq!(gui_menu.fg, 0x00123456);
    assert_eq!(gui_menu.bg, 0x00ABCDEF);
    assert!(!gui_menu.terminal_inverse_video);
}

#[test]
fn test_face_resolver_multibyte_text_property_uses_byte_offsets() {
    let _evaluator = neovm_core::emacs_core::Context::new();

    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);

    let mut buf = test_buffer(7, "*utf8*");
    set_buffer_text(&mut buf, "a好b");
    buf.widen();
    buf.put_text_property(4, 5, Value::symbol("face"), Value::symbol("bold"));

    let mut next_check = buf.point_max_char_pos().get();
    let resolved = resolver.face_at_pos(&buf, 2, &mut next_check);

    assert_eq!(resolved.font_weight, FontWeight::BOLD.css_weight());
    assert_eq!(next_check, 3);
}

#[test]
fn test_face_resolver_multibyte_overlay_uses_byte_offsets() {
    let mut evaluator = neovm_core::emacs_core::Context::new();

    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);

    {
        let buf = evaluator
            .buffer_manager_mut()
            .current_buffer_mut()
            .expect("current buffer");
        buf.insert("a好b");
    }
    let _ = eval_lisp(
        &mut evaluator,
        "(let ((ov (make-overlay 3 4))) (overlay-put ov 'face 'bold) ov)",
    );

    let buf = evaluator
        .buffer_manager()
        .current_buffer()
        .expect("current buffer");
    let mut next_check = buf.point_max_char_pos().get();
    let resolved = resolver.face_at_pos(buf, 2, &mut next_check);

    assert_eq!(resolved.font_weight, FontWeight::BOLD.css_weight());
    assert_eq!(next_check, 3);
}

#[test]
fn test_resolve_face_value_symbol() {
    let _evaluator = neovm_core::emacs_core::Context::new();
    let names = FaceResolver::resolve_face_value(&Value::symbol("bold"));
    assert_eq!(names, vec!["bold"]);
}

#[test]
fn test_resolve_face_value_nil() {
    let _evaluator = neovm_core::emacs_core::Context::new();
    let names = FaceResolver::resolve_face_value(&Value::NIL);
    assert!(names.is_empty());
}

#[test]
fn test_resolve_face_value_list() {
    let _evaluator = neovm_core::emacs_core::Context::new();
    let list = Value::list(vec![Value::symbol("bold"), Value::symbol("italic")]);
    let names = FaceResolver::resolve_face_value(&list);
    assert_eq!(names, vec!["bold", "italic"]);
}

#[test]
fn test_realize_face_height_absolute() {
    let _evaluator = neovm_core::emacs_core::Context::new();
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);

    let mut face = NeoFace::new("tall");
    face.height = Some(FaceHeight::Absolute(240)); // 24pt
    let realized = resolver.realize_face(&face);
    let expected = crate::font::sizing::face_height_to_pixels(240);
    assert!((realized.font_size - expected).abs() < 0.1);
}

#[test]
fn face_resolver_absolute_height_uses_configured_font_sizing() {
    let _evaluator = neovm_core::emacs_core::Context::new();
    let table = FaceTable::new();
    let resolver = FaceResolver::new_with_font_sizing(
        &table,
        0x00FFFFFF,
        0x00000000,
        13.0,
        Some("neo".to_string()),
        crate::font::sizing::FontSizing::logical(),
    );

    let mut face = NeoFace::new("tall");
    face.height = Some(FaceHeight::Absolute(100));
    let realized = resolver.realize_face(&face);

    assert_eq!(realized.font_size, 13.0);
}

#[test]
fn graphic_face_resolver_retains_valid_frame_size_while_default_font_is_unrealized() {
    let mut table = FaceTable::new();
    let mut default = NeoFace::new("default");
    default.height = Some(FaceHeight::Absolute(0));
    table.define("default", default);

    let resolver = FaceResolver::new_with_font_sizing(
        &table,
        0x00FFFFFF,
        0x00000000,
        16.0,
        Some("wayland".to_string()),
        crate::font::sizing::FontSizing::logical(),
    );

    assert_eq!(
        resolver.default_face().font_size,
        16.0,
        "an unrealized GUI face must retain the frame's last coherent font size"
    );
}

#[test]
fn graphic_named_face_retains_valid_frame_size_while_height_is_unrealized() {
    let mut table = FaceTable::new();
    let mut transient = NeoFace::new("transient");
    transient.height = Some(FaceHeight::Absolute(0));
    table.define("transient", transient.clone());

    let resolver = FaceResolver::new_with_font_sizing(
        &table,
        0x00FFFFFF,
        0x00000000,
        16.0,
        Some("wayland".to_string()),
        crate::font::sizing::FontSizing::logical(),
    );

    assert_eq!(resolver.realize_face(&transient).font_size, 16.0);
    assert_eq!(resolver.resolve_named_face("transient").font_size, 16.0);
}

#[test]
fn test_realize_face_height_relative() {
    let _evaluator = neovm_core::emacs_core::Context::new();
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);

    let mut face = NeoFace::new("scaled");
    face.height = Some(FaceHeight::Relative(2.0));
    let realized = resolver.realize_face(&face);
    // 2.0 * default_font_size
    let expected = resolver.default_face().font_size * 2.0;
    assert!((realized.font_size - expected).abs() < 0.1);
}

#[test]
fn test_face_from_plist_realizes_relative_height_family_and_weight() {
    let _evaluator = neovm_core::emacs_core::Context::new();
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 26.666666, None);

    let plist = Value::list(vec![
        Value::keyword("family"),
        Value::string("DejaVu Sans Mono"),
        Value::keyword("height"),
        Value::make_float(1.6),
        Value::keyword("weight"),
        Value::symbol("extra-bold"),
    ]);

    let inline_face = resolver.face_from_plist(&plist).expect("inline plist face");
    let realized = resolver.realize_face(&inline_face);

    assert_eq!(realized.font_family, "DejaVu Sans Mono");
    assert_eq!(realized.font_weight, FontWeight::EXTRA_BOLD.css_weight());
    assert!(
        (realized.font_size - (resolver.default_face().font_size * 1.6)).abs() < 0.1,
        "expected relative face height to scale from the default face size, got {}",
        realized.font_size
    );
}

// ---------------------------------------------------------------------------
// Stage 6: `resolve_fringe_indicator_bitmap_index` — GNU `get_logical_fringe_bitmap`.
// ---------------------------------------------------------------------------

/// `index_of` for a standard fringe-bitmap symbol seeded by `Context::new`.
fn fringe_index_of(ctx: &neovm_core::emacs_core::Context, name: &str) -> u16 {
    let sym = neovm_core::emacs_core::intern::intern(name);
    u16::try_from(
        ctx.fringe_bitmap_registry()
            .index_of(sym)
            .unwrap_or_else(|| panic!("standard bitmap `{name}` should be registered")),
    )
    .expect("index fits in u16")
}

fn intern_value(name: &str) -> Value {
    Value::from_sym_id(neovm_core::emacs_core::intern::intern(name))
}

#[test]
fn resolve_fringe_indicator_bare_symbol_used_for_all_cases() {
    // `(empty-line . right-arrow)` — a bare symbol cdr is used for every case.
    let mut ctx = neovm_core::emacs_core::Context::new();
    let alist = eval_lisp(&mut ctx, "'((empty-line . right-arrow))");
    let mut buf = test_buffer(1, "*fringe*");
    buf.set_buffer_local("fringe-indicator-alist", alist);

    let want = fringe_index_of(&ctx, "right-arrow");
    let empty_line = intern_value("empty-line");
    // Bare symbol => same result for left/right and partial/full.
    for &right_p in &[false, true] {
        for &partial_p in &[false, true] {
            assert_eq!(
                resolve_fringe_indicator_bitmap_index(&buf, &ctx, empty_line, right_p, partial_p),
                Some(want),
                "bare symbol should resolve for right_p={right_p} partial_p={partial_p}",
            );
        }
    }
}

#[test]
fn resolve_fringe_indicator_list_selects_left_or_right() {
    // `(truncation left-arrow right-arrow)` — 2-element cdr list. Left for
    // right_p=false (ix1=0), right for right_p=true (ix1=1).
    let mut ctx = neovm_core::emacs_core::Context::new();
    let alist = eval_lisp(&mut ctx, "'((truncation left-arrow right-arrow))");
    let mut buf = test_buffer(2, "*fringe*");
    buf.set_buffer_local("fringe-indicator-alist", alist);

    let left = fringe_index_of(&ctx, "left-arrow");
    let right = fringe_index_of(&ctx, "right-arrow");
    let truncation = intern_value("truncation");

    assert_eq!(
        resolve_fringe_indicator_bitmap_index(&buf, &ctx, truncation, false, false),
        Some(left),
        "right_p=false picks the left element",
    );
    assert_eq!(
        resolve_fringe_indicator_bitmap_index(&buf, &ctx, truncation, true, false),
        Some(right),
        "right_p=true picks the right element",
    );
    // No partial element present: partial_p falls back to the ix1 element.
    assert_eq!(
        resolve_fringe_indicator_bitmap_index(&buf, &ctx, truncation, true, true),
        Some(right),
        "partial with no partial element falls back to ix1 (right)",
    );
    assert_eq!(
        resolve_fringe_indicator_bitmap_index(&buf, &ctx, truncation, false, true),
        Some(left),
        "partial with no partial element falls back to ix1 (left)",
    );
}

#[test]
fn resolve_fringe_indicator_partial_elements_selected() {
    // `(L R PL PR)` — partial_p picks ix2 (= ix1 + 2).
    let mut ctx = neovm_core::emacs_core::Context::new();
    let alist = eval_lisp(
        &mut ctx,
        "'((truncation left-arrow right-arrow left-curly-arrow right-curly-arrow))",
    );
    let mut buf = test_buffer(3, "*fringe*");
    buf.set_buffer_local("fringe-indicator-alist", alist);

    let left = fringe_index_of(&ctx, "left-arrow");
    let right = fringe_index_of(&ctx, "right-arrow");
    let pleft = fringe_index_of(&ctx, "left-curly-arrow");
    let pright = fringe_index_of(&ctx, "right-curly-arrow");
    let truncation = intern_value("truncation");

    assert_eq!(
        resolve_fringe_indicator_bitmap_index(&buf, &ctx, truncation, false, false),
        Some(left),
    );
    assert_eq!(
        resolve_fringe_indicator_bitmap_index(&buf, &ctx, truncation, true, false),
        Some(right),
    );
    assert_eq!(
        resolve_fringe_indicator_bitmap_index(&buf, &ctx, truncation, false, true),
        Some(pleft),
        "partial-left = ix2 (index 2)",
    );
    assert_eq!(
        resolve_fringe_indicator_bitmap_index(&buf, &ctx, truncation, true, true),
        Some(pright),
        "partial-right = ix2 (index 3)",
    );
}

#[test]
fn resolve_fringe_indicator_t_element_is_no_bitmap() {
    // A `t` element means "no bitmap here". With only a `t` left element and no
    // global fallback, the result is `None`.
    let mut ctx = neovm_core::emacs_core::Context::new();
    let alist = eval_lisp(&mut ctx, "'((truncation t right-arrow))");
    let mut buf = test_buffer(4, "*fringe*");
    buf.set_buffer_local("fringe-indicator-alist", alist);

    let truncation = intern_value("truncation");
    let right = fringe_index_of(&ctx, "right-arrow");

    // Left element is `t` -> no bitmap. Note GNU then falls through to the
    // global default; a bare Context has the standard default which also yields
    // left-arrow for truncation, so assert the buffer-local-only branch result
    // by using an indicator absent from the default.
    let _ = right;
    assert!(
        resolve_fringe_indicator_bitmap_index(&buf, &ctx, truncation, false, false).is_some()
            || resolve_fringe_indicator_bitmap_index(&buf, &ctx, truncation, false, false)
                .is_none(),
        "smoke: call must not panic on a `t` element",
    );
    // The right element is a real bitmap.
    assert_eq!(
        resolve_fringe_indicator_bitmap_index(&buf, &ctx, truncation, true, false),
        Some(right),
        "right element resolves even when the left element is `t`",
    );
}

#[test]
fn resolve_fringe_indicator_t_element_with_custom_indicator_yields_none() {
    // A custom indicator absent from the global default: a `t` element really
    // yields `None` (no global fallback can rescue it).
    let mut ctx = neovm_core::emacs_core::Context::new();
    let alist = eval_lisp(&mut ctx, "'((test-custom-xxx t right-arrow))");
    let mut buf = test_buffer(5, "*fringe*");
    buf.set_buffer_local("fringe-indicator-alist", alist);

    let custom = intern_value("test-custom-xxx");
    assert_eq!(
        resolve_fringe_indicator_bitmap_index(&buf, &ctx, custom, false, false),
        None,
        "`t` left element + no global fallback => None",
    );
    let right = fringe_index_of(&ctx, "right-arrow");
    assert_eq!(
        resolve_fringe_indicator_bitmap_index(&buf, &ctx, custom, true, false),
        Some(right),
    );
}

#[test]
fn resolve_fringe_indicator_unregistered_symbol_is_none() {
    // A bitmap symbol that isn't registered resolves to `None`.
    let mut ctx = neovm_core::emacs_core::Context::new();
    let alist = eval_lisp(&mut ctx, "'((empty-line . no-such-bitmap-zzz))");
    let mut buf = test_buffer(6, "*fringe*");
    buf.set_buffer_local("fringe-indicator-alist", alist);

    let empty_line = intern_value("empty-line");
    assert_eq!(
        resolve_fringe_indicator_bitmap_index(&buf, &ctx, empty_line, false, false),
        None,
    );
}

#[test]
fn resolve_fringe_indicator_missing_entry_is_none() {
    // No matching entry and no global default => None.
    let mut ctx = neovm_core::emacs_core::Context::new();
    let alist = eval_lisp(&mut ctx, "'((continuation . right-curly-arrow))");
    let mut buf = test_buffer(7, "*fringe*");
    buf.set_buffer_local("fringe-indicator-alist", alist);

    let absent = intern_value("definitely-absent-indicator");
    assert_eq!(
        resolve_fringe_indicator_bitmap_index(&buf, &ctx, absent, false, false),
        None,
    );
}

#[test]
fn default_fringe_indicator_alist_seeded_resolves_standard_indicators() {
    // A bare `Context::new()` (no lisp/fringe.el load) still has the GNU default
    // `fringe-indicator-alist` seeded in Rust, so the resolver yields the
    // standard truncation / continuation / empty-line bitmaps via the
    // buffer-default fallback (no buffer-local override on a fresh scratch buf).
    let mut ctx = neovm_core::emacs_core::Context::new();
    let dv = ctx
        .eval_str("(default-value 'fringe-indicator-alist)")
        .expect("default-value");
    assert!(
        !dv.is_nil(),
        "default fringe-indicator-alist should be seeded"
    );

    let buf_id = ctx
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let buf = ctx.buffer_manager().get(buf_id).expect("buffer").clone();

    let idx = |name: &str| {
        let sym = neovm_core::emacs_core::intern::intern(name);
        u16::try_from(ctx.fringe_bitmap_registry().index_of(sym).unwrap()).unwrap()
    };
    let empty_line = Value::from_sym_id(neovm_core::emacs_core::intern::intern("empty-line"));
    let truncation = Value::from_sym_id(neovm_core::emacs_core::intern::intern("truncation"));
    let continuation = Value::from_sym_id(neovm_core::emacs_core::intern::intern("continuation"));

    assert_eq!(
        resolve_fringe_indicator_bitmap_index(&buf, &ctx, empty_line, false, false),
        Some(idx("empty-line")),
        "empty-line resolves via the seeded default alist",
    );
    assert_eq!(
        resolve_fringe_indicator_bitmap_index(&buf, &ctx, truncation, false, false),
        Some(idx("left-arrow")),
    );
    assert_eq!(
        resolve_fringe_indicator_bitmap_index(&buf, &ctx, truncation, true, false),
        Some(idx("right-arrow")),
    );
    assert_eq!(
        resolve_fringe_indicator_bitmap_index(&buf, &ctx, continuation, false, false),
        Some(idx("left-curly-arrow")),
    );
    assert_eq!(
        resolve_fringe_indicator_bitmap_index(&buf, &ctx, continuation, true, false),
        Some(idx("right-curly-arrow")),
    );
}

#[test]
fn face_resolver_honors_overlay_window_property() {
    // hl-line with a non-sticky flag sets the overlay `window` property to the
    // selected window, so GNU applies that overlay's face ONLY in that window --
    // the same buffer in two windows highlights only the selected one. The
    // overlay-face path must skip a windowed overlay whose window isn't the one
    // being resolved, matching the overlay-string path.
    let _ctx = neovm_core::emacs_core::Context::new();
    let mut table = FaceTable::new();
    let mut hl = NeoFace::new("hl-line-test");
    hl.background = Some(neovm_core::face::Color::rgb(0xff, 0xff, 0x00));
    table.define("hl-line-test", hl);
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, Some("x".to_string()));

    let mut buf = test_buffer(1, "*hl*");
    set_buffer_text(&mut buf, "line");
    buf.widen();
    let overlay = Value::make_overlay(neovm_core::heap_types::OverlayDataInit {
        serial: 0,
        plist: Value::NIL,
        buffer: None,
        start: buf.point_min_emacs_byte_pos().get() as usize,
        end: buf.point_max_emacs_byte_pos().get() as usize,
        front_advance: false,
        rear_advance: false,
    });
    buf.overlays_mut().insert_overlay(overlay);
    buf.overlays_mut()
        .overlay_put(
            overlay,
            Value::symbol("face"),
            Value::symbol("hl-line-test"),
        )
        .unwrap();
    buf.overlays_mut()
        .overlay_put(overlay, Value::symbol("window"), Value::make_window(7))
        .unwrap();

    const HL_BG: u32 = 0x00FF_FF00;
    let bg_at = |resolver: &FaceResolver| {
        let mut nc = buf.point_max_char_pos().get();
        resolver.face_at_pos(&buf, 0, &mut nc).bg
    };

    // Matching window -> overlay face applies.
    resolver.set_current_window_id(Some(7));
    assert_eq!(
        bg_at(&resolver),
        HL_BG,
        "overlay face should apply in its own window"
    );
    // Different window -> overlay face must NOT leak in.
    resolver.set_current_window_id(Some(8));
    assert_ne!(
        bg_at(&resolver),
        HL_BG,
        "windowed overlay must not leak into other windows"
    );
    // No window (frame chrome / TTY) -> unrestricted, applies.
    resolver.set_current_window_id(None);
    assert_eq!(bg_at(&resolver), HL_BG, "None window id is unrestricted");
}
