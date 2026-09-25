//! P3.5 stage B: silent frames (B1).

use super::*;

/// A 10x4 renderer that has already painted one frame with `text` on row 0
/// and the cursor at (1, 2).
fn painted(silent: bool, text: &str) -> TtyRif {
    let mut rif = TtyRif::new(10, 4);
    rif.set_silent_frames(silent);
    for (col, ch) in text.chars().enumerate() {
        rif.desired.set(0, col, ch, CellAttrs::default(), false);
    }
    rif.cursor_visible = true;
    rif.cursor_row = 1;
    rif.cursor_col = 2;
    rif.diff_and_render();
    rif.take_output();
    rif
}

/// Paint the same content again with the cursor at (ROW, COL) and SHAPE.
fn repaint(rif: &mut TtyRif, text: &str, cursor: Option<(u16, u16)>, shape: TerminalCursorShape) {
    rif.desired.clear(None);
    for (col, ch) in text.chars().enumerate() {
        rif.desired.set(0, col, ch, CellAttrs::default(), false);
    }
    rif.cursor_visible = cursor.is_some();
    if let Some((row, col)) = cursor {
        rif.cursor_row = row;
        rif.cursor_col = col;
    }
    rif.cursor_shape = shape;
    rif.diff_and_render();
}

fn output_string(rif: &mut TtyRif) -> String {
    String::from_utf8(rif.take_output()).expect("ASCII output")
}

#[test]
fn silent_knob_parses_on_and_defaults_off() {
    assert!(!parse_tty_silent_knob(None));
    assert!(!parse_tty_silent_knob(Some("")));
    assert!(!parse_tty_silent_knob(Some("off")));
    assert!(parse_tty_silent_knob(Some("on")));
    assert!(parse_tty_silent_knob(Some(" 1 ")));
}

#[test]
fn idle_frame_writes_nothing_when_silent() {
    let mut rif = painted(true, "hello");
    repaint(&mut rif, "hello", Some((1, 2)), TerminalCursorShape::Block);
    assert_eq!(output_string(&mut rif), "");
    assert_eq!(rif.frame_stats().bytes, 0);
    assert_eq!(rif.current.cells[0].ch, 'h', "the screen model is kept");
}

#[test]
fn idle_frame_keeps_the_old_framing_when_the_knob_is_off() {
    let mut rif = painted(false, "hello");
    repaint(&mut rif, "hello", Some((1, 2)), TerminalCursorShape::Block);
    let out = output_string(&mut rif);
    assert!(out.contains("\x1b[?25l"), "hide: {out:?}");
    assert!(out.contains("\x1b[2;3H"), "goto: {out:?}");
    assert!(out.contains("\x1b[2 q"), "shape every frame: {out:?}");
    assert!(out.contains("\x1b[?25h"), "show: {out:?}");
}

#[test]
fn cursor_motion_alone_writes_only_the_motion() {
    let mut rif = painted(true, "hello");
    repaint(&mut rif, "hello", Some((2, 5)), TerminalCursorShape::Block);
    assert_eq!(output_string(&mut rif), "\x1b[3;6H");
}

#[test]
fn cursor_shape_change_alone_writes_only_the_shape() {
    let mut rif = painted(true, "hello");
    repaint(&mut rif, "hello", Some((1, 2)), TerminalCursorShape::Bar);
    assert_eq!(output_string(&mut rif), "\x1b[6 q");
    repaint(&mut rif, "hello", Some((1, 2)), TerminalCursorShape::Bar);
    assert_eq!(output_string(&mut rif), "", "the new shape is remembered");
}

#[test]
fn cursor_visibility_changes_are_written_once() {
    let mut rif = painted(true, "hello");
    repaint(&mut rif, "hello", None, TerminalCursorShape::Block);
    assert_eq!(output_string(&mut rif), "\x1b[?25l");
    repaint(&mut rif, "hello", None, TerminalCursorShape::Block);
    assert_eq!(output_string(&mut rif), "");
    repaint(&mut rif, "hello", Some((1, 2)), TerminalCursorShape::Block);
    assert_eq!(output_string(&mut rif), "\x1b[2;3H\x1b[?25h");
}

#[test]
fn a_frame_with_cell_writes_resends_the_shape_only_when_it_changed() {
    let mut rif = painted(true, "hello");
    repaint(&mut rif, "hellO", Some((1, 2)), TerminalCursorShape::Block);
    let out = output_string(&mut rif);
    assert!(out.contains('O'), "the changed cell is written: {out:?}");
    assert!(
        !out.contains("\x1b[2 q"),
        "unchanged shape not re-sent: {out:?}"
    );
    assert!(out.contains("\x1b[?25l") && out.contains("\x1b[?25h"));
    repaint(
        &mut rif,
        "hello",
        Some((1, 2)),
        TerminalCursorShape::Underline,
    );
    let out = output_string(&mut rif);
    assert!(out.contains("\x1b[4 q"), "changed shape re-sent: {out:?}");
}

#[test]
fn a_forced_redraw_forgets_the_terminal_cursor() {
    let mut rif = painted(true, "hello");
    rif.force_redraw();
    repaint(&mut rif, "hello", Some((1, 2)), TerminalCursorShape::Block);
    let out = output_string(&mut rif);
    assert!(out.contains("hello"), "full repaint: {out:?}");
    assert!(
        out.contains("\x1b[2 q"),
        "shape re-sent after a redraw: {out:?}"
    );
    repaint(&mut rif, "hello", Some((1, 2)), TerminalCursorShape::Block);
    assert_eq!(output_string(&mut rif), "");
}

// ---------------------------------------------------------------------------
// B2: the damage path against the full path
// ---------------------------------------------------------------------------

use neomacs_display_protocol::face::Face;
use neomacs_display_protocol::frame_glyphs::{CursorStyle, DisplaySlotId, PhysCursor};
use neomacs_display_protocol::glyph_matrix::{
    FaceFillItem, Glyph, GlyphMatrix, GlyphRow, MatrixRow, RowDamage, WindowMatrixEntry,
};
use neomacs_display_protocol::types::{Color, DisplayFrameId, Px};

#[test]
fn damage_knob_parses_modes_and_defaults_off() {
    assert_eq!(parse_tty_damage_knob(None), TtyDamageMode::Off);
    assert_eq!(parse_tty_damage_knob(Some("off")), TtyDamageMode::Off);
    assert_eq!(parse_tty_damage_knob(Some("on")), TtyDamageMode::On);
    assert_eq!(
        parse_tty_damage_knob(Some(" Verify ")),
        TtyDamageMode::Verify
    );
    assert_eq!(parse_tty_damage_knob(Some("bogus")), TtyDamageMode::Off);
}

fn text_row(role: GlyphRowRole, text: &str, face: u32) -> MatrixRow {
    let mut row = GlyphRow::new(role);
    for (i, ch) in text.chars().enumerate() {
        let mut glyph = Glyph::char(ch, FaceId::new(face), i);
        if neovm_core::encoding::char_width(ch) == 2 {
            glyph.wide = true;
        }
        row.glyphs[GlyphArea::Text as usize].push(glyph);
    }
    row.ends_at_zv = text.is_empty();
    MatrixRow::new(row)
}

/// One leaf window of a [`Scene`]: rows of text then, if `mode_line`, a
/// mode line as its last row. Rows keep their `Arc` from frame to frame
/// unless replaced, exactly as layout reuse does.
#[derive(Clone)]
struct SceneWindow {
    id: i64,
    left: usize,
    top: usize,
    width: usize,
    rows: Vec<MatrixRow>,
    damage: Vec<RowDamage>,
    mode_line: bool,
    selected: bool,
}

impl SceneWindow {
    fn new(
        id: i64,
        left: usize,
        top: usize,
        width: usize,
        lines: &[&str],
        mode_line: bool,
    ) -> Self {
        let mut rows: Vec<MatrixRow> = lines
            .iter()
            .map(|line| text_row(GlyphRowRole::Text, line, 0))
            .collect();
        if mode_line {
            rows.push(text_row(
                GlyphRowRole::ModeLine,
                &format!("-- window {id} --"),
                2,
            ));
        }
        let damage = vec![RowDamage::New; rows.len()];
        Self {
            id,
            left,
            top,
            width,
            rows,
            damage,
            mode_line,
            selected: id == 1,
        }
    }

    /// Start a frame: every row is reused unless replaced below.
    fn reuse_all(&mut self) {
        self.damage
            .iter_mut()
            .for_each(|damage| *damage = RowDamage::Reused);
    }

    fn set_line(&mut self, idx: usize, text: &str, face: u32) {
        let role = if self.mode_line && idx + 1 == self.rows.len() {
            GlyphRowRole::ModeLine
        } else {
            GlyphRowRole::Text
        };
        self.rows[idx] = text_row(role, text, face);
        self.damage[idx] = RowDamage::New;
    }

    fn entry(&self) -> WindowMatrixEntry {
        let height = self.rows.len();
        let mut matrix = GlyphMatrix::new(height, self.width);
        for (idx, row) in self.rows.iter().enumerate() {
            matrix.rows[idx] = row.clone();
            matrix.set_row_damage(idx, self.damage[idx]);
        }
        let bounds = Rect::new(
            self.left as f32,
            self.top as f32,
            self.width as f32,
            height as f32,
        );
        WindowMatrixEntry {
            window_id: DisplayWindowId::new(self.id),
            matrix,
            pixel_bounds: bounds,
            text_pixel_bounds: bounds,
            text_clip_bounds: None,
            selected: self.selected,
        }
    }
}

#[derive(Clone)]
struct Scene {
    cols: usize,
    rows: usize,
    windows: Vec<SceneWindow>,
    fills: Vec<FaceFillItem>,
    faces: FrameFaceMap,
    cursor: Option<(usize, usize)>,
    child: Option<(f32, f32, String)>,
}

fn face(id: u32, bg: Option<u16>, fg: Option<u16>) -> Face {
    let mut face = Face::new(FaceId::new(id));
    face.terminal_background = bg.map(TerminalColor::Indexed);
    face.terminal_foreground = fg.map(TerminalColor::Indexed);
    face.use_default_background = bg.is_none();
    face.use_default_foreground = fg.is_none();
    face
}

impl Scene {
    fn new(cols: usize, rows: usize) -> Self {
        let mut faces = FrameFaceMap::default();
        faces.insert(FaceId::new(0), face(0, None, None));
        faces.insert(FaceId::new(1), face(1, Some(4), Some(15)));
        faces.insert(FaceId::new(2), face(2, Some(7), Some(0)));
        faces.insert(FaceId::new(3), face(3, Some(22), None));
        Self {
            cols,
            rows,
            windows: Vec::new(),
            fills: Vec::new(),
            faces,
            cursor: Some((0, 0)),
            child: None,
        }
    }

    fn window(&mut self, id: i64) -> &mut SceneWindow {
        self.windows
            .iter_mut()
            .find(|window| window.id == id)
            .expect("scene window")
    }

    fn next_frame(&mut self) {
        self.windows.iter_mut().for_each(SceneWindow::reuse_all);
    }

    /// Relay every row: equal content, new identities.
    fn relay_all(&mut self) {
        for window in &mut self.windows {
            for idx in 0..window.rows.len() {
                let row: GlyphRow = (*window.rows[idx]).clone();
                window.rows[idx] = MatrixRow::new(row);
                window.damage[idx] = RowDamage::New;
            }
        }
    }

    fn fill(&mut self, window: i64, top: usize, rows: usize, face: u32) {
        let owner = self
            .windows
            .iter()
            .find(|candidate| candidate.id == window)
            .expect("fill window");
        let bounds = Rect::new(
            owner.left as f32,
            top as f32,
            owner.width as f32,
            rows as f32,
        );
        self.fills.push(FaceFillItem {
            window_id: DisplayWindowId::new(window),
            row_role: GlyphRowRole::Text,
            clip_rect: None,
            bounds,
            face_id: FaceId::new(face),
        });
    }

    fn placed(state: &mut FrameDisplayState, frame: u64, parent: Option<u64>, x: f32, y: f32) {
        state.frame_placement = neomacs_display_protocol::PresentedFramePlacement::new(
            DisplayFrameId::new(frame),
            state.presentation_id,
            parent.map(DisplayFrameId::new),
            neomacs_display_protocol::ParentFrameRect::new(
                x,
                y,
                state.frame_pixel_width,
                state.frame_pixel_height,
            )
            .expect("frame rect"),
            0,
        );
    }

    fn states(&self) -> (FrameDisplayState, Vec<FrameDisplayState>) {
        let mut state = FrameDisplayState::new(self.cols, self.rows, 1.0, 1.0);
        Self::placed(&mut state, 1, None, 0.0, 0.0);
        state.background = Color::BLACK;
        state.faces = self.faces.clone();
        state.face_fills = self.fills.clone();
        state.window_matrices = self.windows.iter().map(SceneWindow::entry).collect();
        if let Some((col, row)) = self.cursor {
            state.phys_cursor = Some(PhysCursor {
                window_id: DisplayWindowId::new(1),
                charpos: 1,
                row,
                col: col as u16,
                x: col as f32,
                y: row as f32,
                width: 1.0,
                height: 1.0,
                ascent: 1.0,
                style: CursorStyle::FilledBox,
                color: Color::WHITE,
                slot_id: DisplaySlotId {
                    window_id: DisplayWindowId::new(1),
                    row: row as u32,
                    col: col as u16,
                },
                cursor_fg: Color::BLACK,
            });
        }
        let mut children = Vec::new();
        if let Some((x, y, text)) = &self.child {
            let width = text.chars().count().max(1);
            let mut child = FrameDisplayState::new(width, 1, 1.0, 1.0);
            Self::placed(&mut child, 2, Some(1), *x, *y);
            child.background = Color::BLACK;
            child.faces = self.faces.clone();
            let mut window = SceneWindow::new(20, 0, 0, width, &[text], false);
            window.selected = true;
            child.window_matrices.push(window.entry());
            children.push(child);
        }
        (state, children)
    }
}

/// Three renderers fed the same frames: the full path, the damage path, and
/// `verify`. After every frame the damage path must have written the same
/// bytes and left the same screen model as the full path.
struct Differential {
    full: TtyRif,
    damage: TtyRif,
    verify: TtyRif,
    damage_frames: usize,
    churn_frames: usize,
    verify_screen_diffs_accepted: u64,
    repainted: Vec<u32>,
}

impl Differential {
    fn new(scene: &Scene, silent: bool) -> Self {
        let make = |mode| {
            let mut rif = TtyRif::new(scene.cols, scene.rows);
            rif.set_silent_frames(silent);
            rif.set_damage_mode(mode);
            rif
        };
        Self {
            full: make(TtyDamageMode::Off),
            damage: make(TtyDamageMode::On),
            verify: make(TtyDamageMode::Verify),
            damage_frames: 0,
            churn_frames: 0,
            verify_screen_diffs_accepted: 0,
            repainted: Vec::new(),
        }
    }

    fn each(&mut self, f: impl Fn(&mut TtyRif)) {
        f(&mut self.full);
        f(&mut self.damage);
        f(&mut self.verify);
    }

    /// [`Self::frame`] for a frame the full path renders stale (a closed
    /// child frame over reused rows, see `damage.rs`): the damage path must
    /// match a fresh repaint, and `verify` reports the full path's rows as
    /// screen differences, never as false negatives.
    fn frame_accepting_stale_full_path(&mut self, scene: &Scene, label: &str) {
        let (root, children) = scene.states();
        self.each(|rif| {
            rif.rasterize_frame_tree(&root, &children);
            rif.diff_and_render();
        });
        for rif in [&mut self.full, &mut self.damage, &mut self.verify] {
            rif.take_output();
        }
        let mut fresh = TtyRif::new(scene.cols, scene.rows);
        fresh.rasterize_frame_tree(&root, &children);
        fresh.diff_and_render();
        let mut stale_full_rows = 0;
        for row in 0..scene.rows {
            assert!(
                cells_same_content(self.damage.current.row(row), fresh.current.row(row)),
                "{label}: damage row {row} differs from a fresh repaint"
            );
            stale_full_rows += usize::from(!cells_same_content(
                self.full.current.row(row),
                fresh.current.row(row),
            ));
        }
        let frame = self.verify.frame_stats().verify;
        assert!(
            stale_full_rows > 0,
            "{label}: the full path keeps the child"
        );
        assert_eq!(frame.false_negatives, 0, "{label}");
        assert_eq!(frame.screen_diff_rows as usize, stale_full_rows, "{label}");
        // The full path stays stale; resynchronize the two for the frames that
        // follow, as a redraw would.
        self.full.force_redraw();
        self.verify.force_redraw();
        self.damage.force_redraw();
        self.verify_screen_diffs_accepted += frame.screen_diff_rows;
        self.repainted.push(u32::MAX);
    }

    fn frame(&mut self, scene: &Scene, label: &str) {
        let (root, children) = scene.states();
        self.each(|rif| {
            rif.rasterize_frame_tree(&root, &children);
            rif.diff_and_render();
        });
        let full = self.full.take_output();
        let damage = self.damage.take_output();
        let verified = self.verify.take_output();
        let frame = self.verify.frame_stats().verify;
        // Byte-identical, except where the full path rewrote rows without
        // changing them (its materialization churn, see `damage.rs`).
        if frame.redundant_rewrite_rows == 0 {
            assert_eq!(
                String::from_utf8_lossy(&damage),
                String::from_utf8_lossy(&full),
                "{label}: damage bytes differ from the full path"
            );
        } else {
            self.churn_frames += 1;
            assert!(
                damage.len() < full.len(),
                "{label}: churn costs the full path"
            );
        }
        assert_eq!(
            verified, full,
            "{label}: verify writes the full path's bytes"
        );
        // What a renderer with no history shows for this frame.
        let mut fresh = TtyRif::new(scene.cols, scene.rows);
        fresh.set_damage_mode(TtyDamageMode::Off);
        fresh.rasterize_frame_tree(&root, &children);
        fresh.diff_and_render();
        for row in 0..scene.rows {
            assert!(
                cells_same_content(self.damage.current.row(row), self.full.current.row(row)),
                "{label}: screen row {row} differs from the full path"
            );
            assert!(
                cells_same_content(self.damage.current.row(row), fresh.current.row(row)),
                "{label}: screen row {row} differs from a fresh repaint"
            );
        }
        let stats = self.damage.frame_stats();
        self.damage_frames += usize::from(stats.damage_frame);
        self.repainted.push(if stats.damage_frame {
            stats.rows_repainted
        } else {
            u32::MAX
        });
        let totals = self.verify.damage_verify_totals();
        assert_eq!(totals.false_negatives, 0, "{label}: verify false negatives");
        assert_eq!(
            totals.screen_diff_rows, self.verify_screen_diffs_accepted,
            "{label}: verify screen diffs"
        );
    }
}

/// An editing session over one window plus the echo area: idle frames,
/// typing (including a wide character), cursor motion, a message, a region
/// fill appearing and going, a theme change, a scroll, a split, a child
/// frame, a forced redraw and a resize.
fn editing_session(silent: bool) -> Differential {
    let lines: Vec<String> = (0..9).map(|i| format!("(line {i} of text)")).collect();
    let line_refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let mut scene = Scene::new(40, 12);
    scene
        .windows
        .push(SceneWindow::new(1, 0, 0, 40, &line_refs, true));
    scene
        .windows
        .push(SceneWindow::new(2, 0, 10, 40, &[""], false));
    scene.windows[1].selected = false;
    scene.fill(1, 0, 9, 0);
    let mut diff = Differential::new(&scene, silent);
    diff.frame(&scene, "first frame");

    scene.next_frame();
    diff.frame(&scene, "idle");
    scene.next_frame();
    diff.frame(&scene, "idle again");

    scene.next_frame();
    scene.window(1).set_line(3, "(line 3 of text)x", 0);
    scene.window(1).set_line(9, "-- window 1 -- L4", 2);
    scene.cursor = Some((17, 3));
    diff.frame(&scene, "type a character");

    scene.next_frame();
    scene.window(1).set_line(3, "(line 3 of text)x\u{4e2d}", 0);
    scene.cursor = Some((19, 3));
    diff.frame(&scene, "type a wide character");

    scene.next_frame();
    scene.cursor = Some((5, 4));
    diff.frame(&scene, "cursor motion only");

    scene.next_frame();
    scene.window(2).set_line(0, "Mark set", 0);
    diff.frame(&scene, "echo-area message");

    // The engine publishes one face fill per window, its default background,
    // and relays the window's rows when that face changes.
    scene.next_frame();
    scene.fills.clear();
    scene.fill(1, 0, 9, 3);
    for idx in 0..9 {
        let text = format!("(line {idx} of text)");
        scene.window(1).set_line(idx, &text, 0);
    }
    diff.frame(&scene, "window background changes");

    scene.next_frame();
    scene.window(1).set_line(5, "(line 5 of text) edited", 3);
    diff.frame(&scene, "type on the new background");

    scene.next_frame();
    diff.frame(&scene, "idle on the new background");

    // A face change bumps the engine's face generation, which relays every
    // row (the full path's carry relies on that).
    scene.next_frame();
    scene
        .faces
        .insert(FaceId::new(2), face(2, Some(9), Some(0)));
    scene.relay_all();
    diff.frame(&scene, "theme change");

    scene.next_frame();
    {
        let window = scene.window(1);
        let first = window.rows.remove(0);
        drop(first);
        window
            .rows
            .insert(8, text_row(GlyphRowRole::Text, "(new last line)", 0));
        for idx in 0..8 {
            window.damage[idx] = RowDamage::ReusedShifted { dvpos: Px(-1.0) };
        }
        window.damage[8] = RowDamage::New;
    }
    diff.frame(&scene, "scroll by a line");

    scene.next_frame();
    diff.frame(&scene, "idle after scroll");

    scene.next_frame();
    {
        let window = scene.window(1);
        window.rows.truncate(4);
        window.damage.truncate(4);
        window
            .rows
            .push(text_row(GlyphRowRole::ModeLine, "-- window 1 (top) --", 2));
        window.damage.push(RowDamage::New);
    }
    // The window keeps its background; only the fill's extent shrinks.
    scene.fills.clear();
    scene.fill(1, 0, 4, 3);
    scene.windows.insert(
        1,
        SceneWindow::new(3, 0, 5, 40, &["other", "window", "rows", ""], true),
    );
    scene.window(3).selected = false;
    diff.frame(&scene, "split window");

    scene.next_frame();
    scene.window(3).set_line(1, "window!", 0);
    diff.frame(&scene, "type in the other window");

    scene.next_frame();
    scene.child = Some((10.0, 2.0, "M-x child".to_string()));
    diff.frame(&scene, "child frame appears");
    scene.next_frame();
    diff.frame(&scene, "child frame stays");
    scene.next_frame();
    scene.child = None;
    diff.frame_accepting_stale_full_path(&scene, "child frame goes");

    scene.next_frame();
    diff.each(TtyRif::force_redraw);
    diff.frame(&scene, "forced redraw");

    scene.next_frame();
    diff.frame(&scene, "idle after redraw");
    diff
}

#[test]
fn damage_path_matches_the_full_path_over_an_editing_session() {
    let diff = editing_session(false);
    assert!(
        diff.damage_frames >= 8,
        "the damage path engaged on the small frames: {:?}",
        diff.repainted
    );
    // idle, idle again: nothing repainted; typing: the edited row and the
    // mode line; cursor motion: nothing.
    assert_eq!(diff.repainted[1], 0, "idle: {:?}", diff.repainted);
    assert_eq!(diff.repainted[2], 0, "idle again: {:?}", diff.repainted);
    assert_eq!(diff.repainted[3], 2, "typing: {:?}", diff.repainted);
    assert_eq!(diff.repainted[5], 0, "cursor motion: {:?}", diff.repainted);
    assert_eq!(diff.repainted[6], 1, "message: {:?}", diff.repainted);
    let totals = diff.verify.damage_verify_totals();
    assert_eq!(totals.frames as usize, diff.repainted.len());
    assert!(totals.damage_frames > 0);
}

#[test]
fn damage_path_matches_the_full_path_with_silent_frames() {
    let diff = editing_session(true);
    assert!(diff.damage_frames >= 8, "{:?}", diff.repainted);
}

#[test]
fn a_resize_takes_the_full_path_and_stays_identical() {
    let mut scene = Scene::new(30, 8);
    scene.windows.push(SceneWindow::new(
        1,
        0,
        0,
        30,
        &["one", "two", "three"],
        true,
    ));
    let mut diff = Differential::new(&scene, false);
    diff.frame(&scene, "first");
    scene.next_frame();
    diff.frame(&scene, "idle");
    scene.cols = 34;
    scene.rows = 9;
    diff.each(|rif| rif.resize(34, 9));
    scene.next_frame();
    diff.frame(&scene, "after resize");
    assert_eq!(diff.repainted[2], u32::MAX, "a resize repaints in full");
    scene.next_frame();
    diff.frame(&scene, "idle after resize");
    assert_eq!(diff.repainted[3], 0);
}

#[test]
fn a_new_matrix_row_with_equal_text_still_repaints_its_row() {
    // Identity, not content, is the key: a relaid row is replanned even when
    // its text did not change, and the planner then finds nothing to write.
    let mut scene = Scene::new(20, 5);
    scene
        .windows
        .push(SceneWindow::new(1, 0, 0, 20, &["same", "text"], true));
    let mut diff = Differential::new(&scene, true);
    diff.frame(&scene, "first");
    scene.next_frame();
    scene.window(1).set_line(1, "text", 0);
    diff.frame(&scene, "relaid, equal");
    assert_eq!(diff.repainted[1], 1);
    assert!(diff.damage.frame_stats().bytes == 0, "nothing to write");
}

/// A face fill that changes under rows layout reused is repainted by the
/// damage path (the fill is one of the row's painters). The full path carries
/// the reused rows from the screen model instead, so it keeps the old fill:
/// today's engine never does this (its one fill per window, the default
/// background, changes only with a relayout), which is why the carry is
/// sound in practice; the damage path does not depend on it.
#[test]
fn a_fill_change_under_reused_rows_is_repainted() {
    let lines = ["alpha", "beta", "gamma", "", "", "", "", "", "", "", "", ""];
    let mut scene = Scene::new(20, 14);
    scene
        .windows
        .push(SceneWindow::new(1, 0, 0, 20, &lines, true));
    scene.fill(1, 0, 3, 0);
    let mut full = TtyRif::new(20, 14);
    full.set_damage_mode(TtyDamageMode::Off);
    let mut damage = TtyRif::new(20, 14);
    damage.set_damage_mode(TtyDamageMode::On);
    let render = |rif: &mut TtyRif, scene: &Scene| {
        let (root, children) = scene.states();
        rif.rasterize_frame_tree(&root, &children);
        rif.diff_and_render();
        rif.take_output();
    };
    render(&mut full, &scene);
    render(&mut damage, &scene);

    scene.next_frame();
    scene.fills.clear();
    scene.fill(1, 0, 3, 3);
    render(&mut full, &scene);
    render(&mut damage, &scene);
    assert!(damage.frame_stats().damage_frame);
    assert_eq!(damage.frame_stats().rows_repainted, 3);

    // What a renderer with no history shows for the new frame.
    let mut fresh = TtyRif::new(20, 14);
    fresh.set_damage_mode(TtyDamageMode::Off);
    render(&mut fresh, &scene);
    let fill_bg = Some(TerminalColor::Indexed(22));
    assert_eq!(fresh.current.row(1)[10].attrs.bg, fill_bg);
    for row in 0..14 {
        assert!(
            cells_same_content(damage.current.row(row), fresh.current.row(row)),
            "row {row}"
        );
    }
    assert_ne!(
        full.current.row(1)[10].attrs.bg,
        fill_bg,
        "the full path's carry keeps the old fill (see above)"
    );
}

/// `same_on_a_terminal` must say "alike" only when `resolve_attrs` agrees:
/// it decides whether a face change can leave a reused row stale.
#[test]
fn faces_alike_on_a_terminal_resolve_to_the_same_cell_attrs() {
    use neomacs_display_protocol::face::{FaceAttributes, UnderlineStyle};
    let base = face(5, Some(4), Some(15));
    let variants: Vec<(&str, Face, bool)> = vec![
        (
            "font_ascent",
            {
                let mut f = base.clone();
                f.font_ascent = 1;
                f
            },
            true,
        ),
        (
            "font_size",
            {
                let mut f = base.clone();
                f.font_size = 13.0;
                f
            },
            true,
        ),
        (
            "gui foreground",
            {
                let mut f = base.clone();
                f.foreground = Color::WHITE;
                f
            },
            true,
        ),
        ("terminal background", face(5, Some(5), Some(15)), false),
        ("terminal foreground", face(5, Some(4), Some(14)), false),
        (
            "default background",
            {
                let mut f = base.clone();
                f.use_default_background = true;
                f
            },
            false,
        ),
        (
            "bold",
            {
                let mut f = base.clone();
                f.attributes |= FaceAttributes::BOLD;
                f
            },
            false,
        ),
        (
            "weight",
            {
                let mut f = base.clone();
                f.font_weight = 700;
                f
            },
            false,
        ),
        (
            "underline",
            {
                let mut f = base.clone();
                f.underline_style = UnderlineStyle::Line;
                f
            },
            false,
        ),
        (
            "inverse",
            {
                let mut f = base.clone();
                f.attributes |= FaceAttributes::INVERSE;
                f
            },
            false,
        ),
    ];
    for (what, variant, alike) in variants {
        assert_eq!(same_on_a_terminal(&base, &variant), alike, "{what}");
        let resolve = |face: &Face| {
            let mut rif = TtyRif::new(1, 1);
            let mut faces = FrameFaceMap::default();
            faces.insert(FaceId::new(5), face.clone());
            rif.set_faces(faces);
            rif.resolve_attrs(FaceId::new(5))
        };
        if alike {
            assert_eq!(resolve(&base), resolve(&variant), "{what}");
        }
    }
}
