use super::*;

// Frozen113-bit baseline methods; only their private names/calls are renamed.
impl TtyRif {
    fn reference_blank_erase_for_face(&self, face_id: FaceId) -> BlankErase {
        if self.resolve_attrs(face_id) == self.resolve_attrs(FaceId::new(0)) {
            BlankErase::DefaultFace
        } else {
            BlankErase::Explicit
        }
    }

    fn reference_set_desired_glyph_cell(
        &mut self,
        row: usize,
        col: usize,
        ch: char,
        attrs: CellAttrs,
        padding: bool,
        blank_erase: BlankErase,
    ) {
        let preserved_materialization =
            (blank_erase == BlankErase::DefaultFace && ch == ' ' && !padding)
                .then(|| self.current.cells.get(row * self.current.width + col))
                .flatten()
                .filter(|current| {
                    current.ch() == ' '
                        && current.attrs() == attrs
                        && !current.padding()
                        && current.extenders().is_none()
                        && current.blank_erase() == BlankErase::DefaultFace
                })
                .map(|current| current.materialization());
        self.desired
            .set_with_blank_erase(row, col, ch, attrs, padding, blank_erase);
        if let Some(materialization) = preserved_materialization {
            self.desired.cells[row * self.desired.width + col].set_materialization(materialization);
        }
    }

    fn reference_write_grapheme_cell(
        &mut self,
        row: usize,
        col: &mut i64,
        text: &str,
        attrs: CellAttrs,
    ) {
        let mut chars = text.chars().filter(|c| !is_tty_skippable_format(*c));
        let base = chars.next().unwrap_or(' ');
        let rest: String = chars.collect();
        if let Some(col) = visible_cell(*col, self.desired.width) {
            self.desired
                .set_cluster(row, col, base, &rest, attrs, false);
        }
        *col = col.saturating_add(1);
    }

    fn reference_rasterize_face_fill(
        &mut self,
        origin_col: i64,
        origin_row: i64,
        state: &FrameDisplayState,
        fill: &FaceFillItem,
        rows: Option<&[bool]>,
    ) {
        let char_w = state.char_width.max(1.0);
        let char_h = state.char_height.max(1.0);
        let Some(cell_rect) = projected_face_fill_rect(fill, char_w, char_h)
            .map(|rect| rect.translated(origin_col, origin_row))
        else {
            return;
        };

        let attrs = self.resolve_attrs(fill.face_id);
        let visible_rows =
            visible_cell_range(cell_rect.top, cell_rect.height(), self.desired.height);
        let visible_cols =
            visible_cell_range(cell_rect.left, cell_rect.width(), self.desired.width);
        for row in visible_rows {
            if rows.is_some_and(|rows| !rows.get(row).copied().unwrap_or(false)) {
                continue;
            }
            for col in visible_cols.clone() {
                self.desired.set(row, col, ' ', attrs, false);
            }
        }
    }

    fn reference_blank_cell_attrs(
        &self,
        state: &FrameDisplayState,
        owner: TtyGlyphRowOwner,
        role: GlyphRowRole,
        local_col: i64,
        local_row: i64,
    ) -> CellAttrs {
        let TtyGlyphRowOwner::Window(window_id) = owner else {
            return self.resolve_attrs(FaceId::new(0));
        };
        state
            .face_fills
            .iter()
            .rev()
            .find(|fill| {
                fill.window_id == window_id
                    && fill.row_role == role
                    && projected_face_fill_rect(fill, state.char_width, state.char_height)
                        .is_some_and(|rect| rect.contains(local_col, local_row))
            })
            .map_or_else(
                || self.resolve_attrs(FaceId::new(0)),
                |fill| self.resolve_attrs(fill.face_id),
            )
    }

    fn reference_rasterize_glyph_row(
        &mut self,
        state: &FrameDisplayState,
        owner: TtyGlyphRowOwner,
        frame_origin_col: i64,
        frame_origin_row: i64,
        screen_col_start: i64,
        screen_row: i64,
        glyph_row: &GlyphRow,
        area_layout: GlyphRowAreaLayout,
        char_width: f32,
    ) {
        let Some(screen_row) = visible_cell(screen_row, self.desired.height) else {
            return;
        };
        if !glyph_row.enabled {
            return;
        }

        // The protocol may carry surplus matrix rows for GUI partial-row
        // rasterization.  GNU's TTY matrices are exactly WINDOW_TOTAL_LINES;
        // project the row's explicit clip before touching any glyph area so a
        // surplus text row (including its reserved right border) cannot escape
        // into the next leaf window.
        let local_row = i64::try_from(screen_row)
            .unwrap_or(i64::MAX)
            .saturating_sub(frame_origin_row);
        if let GlyphAreaPlacement::Structural(text_geometry) =
            area_layout.placement(GlyphArea::Text)
            && !TtyCellRect::project(text_geometry.clip(), char_width, state.char_height)
                .contains_row(local_row)
        {
            return;
        }

        // A row's horizontal start and source slot are one authoritative
        // placement.  TTY cells have no sub-cell positioning, so project the
        // pixel offset to its already-resolved display column.
        let mut col = screen_col_start.saturating_add(i64::from(glyph_row.start_col));
        let screen_width = usize_to_i64_saturating(self.desired.width);

        for area in GlyphArea::ALL {
            if let GlyphAreaPlacement::Structural(geometry) = area_layout.placement(area) {
                col = frame_origin_col
                    .saturating_add((geometry.bounds().x / char_width).round() as i64);
                if area == GlyphArea::Text {
                    col = col.saturating_add(i64::from(glyph_row.start_col));
                }
            }
            let glyphs = &glyph_row.glyphs[area.index()];
            let mut glyph_idx = 0;
            let mut preceding_wide_base_visible = None;
            while glyph_idx < glyphs.len() {
                let glyph = &glyphs[glyph_idx];
                if col >= screen_width {
                    break;
                }

                if glyph.padding {
                    let attrs = self.resolve_attrs(glyph.face_id);
                    if let Some(col) = visible_cell(col, self.desired.width) {
                        self.reference_set_desired_glyph_cell(
                            screen_row,
                            col,
                            ' ',
                            attrs,
                            preceding_wide_base_visible.take().unwrap_or(true),
                            BlankErase::Explicit,
                        );
                    }
                    col = col.saturating_add(1);
                    glyph_idx += 1;
                    continue;
                }

                let attrs = self.resolve_attrs(glyph.face_id);
                let base_visible = visible_cell(col, self.desired.width).is_some();
                // Composite glyphs (base char + grapheme-cluster
                // extenders) occupy one cell whose content is the full
                // cluster string, mirroring GNU's COMPOSITE_GLYPH.
                match &glyph.glyph_type {
                    GlyphType::AutomaticComposite { text, terminal } => {
                        let direction = glyph.resolved_bidi_direction();
                        for visual_offset in 0..terminal.cells.len() {
                            let cell = terminal
                                .visual_cell(direction, visual_offset)
                                .expect("visual terminal-cell offset is in bounds");
                            if col >= screen_width {
                                break;
                            }
                            let base_col = col;
                            if let Some(visible_col) = visible_cell(col, self.desired.width) {
                                self.desired.set_cluster(
                                    screen_row,
                                    visible_col,
                                    cell.base,
                                    &cell.extenders,
                                    attrs,
                                    false,
                                );
                            }
                            col = col.saturating_add(1);
                            for _ in 1..cell.width_cols {
                                if col >= screen_width {
                                    break;
                                }
                                if let Some(visible_col) = visible_cell(col, self.desired.width) {
                                    self.reference_set_desired_glyph_cell(
                                        screen_row,
                                        visible_col,
                                        ' ',
                                        attrs,
                                        visible_cell(base_col, self.desired.width).is_some(),
                                        BlankErase::Explicit,
                                    );
                                }
                                col = col.saturating_add(1);
                            }
                        }

                        // Layout retains one source-position padding glyph per
                        // remaining character.  The terminal plan above owns
                        // their cells, so consume those metadata-only members.
                        let source_paddings = text.chars().count().saturating_sub(1);
                        let present_paddings = glyphs[glyph_idx + 1..]
                            .iter()
                            .take(source_paddings)
                            .take_while(|next| is_run_member_padding_cell(next))
                            .count();
                        glyph_idx += present_paddings;
                    }
                    GlyphType::Composite { text } => {
                        // A contextual-shaping run (Arabic, Indic) is the base
                        // Composite followed by one per-letter grapheme padding
                        // cell per following letter. The GUI shapes the whole
                        // run from the base Composite, but a terminal cannot —
                        // so lay the run out one grapheme per column, visually
                        // reversed for right-to-left, mirroring GNU's term.c.
                        // A plain grapheme cluster (emoji, base+combining) has no
                        // such grapheme paddings and stays a single cell.
                        let run_paddings: Vec<String> = glyphs[glyph_idx + 1..]
                            .iter()
                            .take_while(|g| is_run_member_padding_cell(g))
                            .map(cell_grapheme_string)
                            .collect();
                        if run_paddings.is_empty() {
                            self.reference_write_grapheme_cell(screen_row, &mut col, text, attrs);
                        } else {
                            // Paddings hold the run's letters after the base, in
                            // logical order; the base cell's own grapheme is the
                            // run text with that suffix removed.
                            let tail: String = run_paddings.concat();
                            let g0 = text.strip_suffix(tail.as_str()).unwrap_or(text);
                            let mut graphemes: Vec<&str> =
                                Vec::with_capacity(run_paddings.len() + 1);
                            graphemes.push(g0);
                            graphemes.extend(run_paddings.iter().map(String::as_str));
                            match glyph.resolved_bidi_direction() {
                                ResolvedBidiDirection::LeftToRight => {}
                                ResolvedBidiDirection::RightToLeft => graphemes.reverse(),
                            }
                            let consumed = graphemes.len() - 1;
                            for grapheme in graphemes {
                                if col >= screen_width {
                                    break;
                                }
                                self.reference_write_grapheme_cell(
                                    screen_row, &mut col, grapheme, attrs,
                                );
                            }
                            glyph_idx += consumed;
                        }
                    }
                    GlyphType::Stretch { width_cols } => {
                        let width_cols = usize::from((*width_cols).max(1));
                        for _ in 0..width_cols {
                            if col >= screen_width {
                                break;
                            }
                            if let Some(col) = visible_cell(col, self.desired.width) {
                                self.reference_set_desired_glyph_cell(
                                    screen_row,
                                    col,
                                    ' ',
                                    attrs,
                                    false,
                                    self.reference_blank_erase_for_face(glyph.face_id),
                                );
                            }
                            col = col.saturating_add(1);
                        }
                    }
                    GlyphType::Surface { width_cols, .. } => {
                        // A shader surface is GPU-only; a terminal cannot draw
                        // it. Fill its reserved columns with a visible labeled
                        // placeholder instead of blank space (surfaces are a
                        // neomacs extension, so there is no GNU TTY behavior to
                        // match). This also occupies the full width_cols, which
                        // the single-char fallthrough arm would not.
                        let width_cols = usize::from((*width_cols).max(1));
                        for ch in surface_tty_placeholder(width_cols).chars() {
                            if col >= screen_width {
                                break;
                            }
                            if let Some(col) = visible_cell(col, self.desired.width) {
                                self.reference_set_desired_glyph_cell(
                                    screen_row,
                                    col,
                                    ch,
                                    attrs,
                                    false,
                                    BlankErase::Explicit,
                                );
                            }
                            col = col.saturating_add(1);
                        }
                    }
                    GlyphType::Glyphless { presentation, .. } => {
                        use neomacs_display_protocol::glyph_matrix::GlyphlessPresentation;
                        match presentation {
                            // GNU term.c `produce_glyphless_glyph`: graphical
                            // thin-space is one pixel, but a terminal has no
                            // sub-cell space and emits one canonical blank.
                            GlyphlessPresentation::ThinSpace => {
                                if let Some(col) = visible_cell(col, self.desired.width) {
                                    self.reference_set_desired_glyph_cell(
                                        screen_row,
                                        col,
                                        ' ',
                                        attrs,
                                        false,
                                        BlankErase::Explicit,
                                    );
                                }
                                col = col.saturating_add(1);
                            }
                            // These presentations retain their source scalar
                            // and method in the protocol. Their complete boxed
                            // terminal lowering is intentionally centralized
                            // here rather than inferred from pixel width.
                            GlyphlessPresentation::EmptyBox | GlyphlessPresentation::HexCode => {
                                let ch = glyph_to_char(glyph);
                                if let Some(col) = visible_cell(col, self.desired.width) {
                                    self.reference_set_desired_glyph_cell(
                                        screen_row,
                                        col,
                                        ch,
                                        attrs,
                                        false,
                                        BlankErase::Explicit,
                                    );
                                }
                                col = col.saturating_add(1);
                            }
                        }
                    }
                    GlyphType::Char { ch } if neovm_core::encoding::char_width(*ch) == 0 => {
                        // GNU `append_glyph` appends no terminal glyph for an
                        // ordinary zero-width character.  Only a character
                        // captured inside an automatic composition can acquire
                        // a base cell or extend the previous composed cell.
                    }
                    _ => {
                        // A wide base in the final screen column has no room
                        // for its padding half: the terminal blanks it while
                        // the model would keep it. GNU never emits a
                        // partially visible multi-column glyph; write the
                        // space it would show instead.
                        let ch = if glyph.wide && col + 1 >= screen_width {
                            ' '
                        } else {
                            glyph_to_char(glyph)
                        };
                        if let Some(col) = visible_cell(col, self.desired.width) {
                            self.reference_set_desired_glyph_cell(
                                screen_row,
                                col,
                                ch,
                                attrs,
                                false,
                                if ch == ' ' {
                                    self.reference_blank_erase_for_face(glyph.face_id)
                                } else {
                                    BlankErase::Explicit
                                },
                            );
                        }
                        col = col.saturating_add(1);

                        let next_is_explicit_padding = glyph.wide
                            && glyphs
                                .get(glyph_idx + 1)
                                .is_some_and(|next_glyph| next_glyph.padding);
                        if glyph.wide && !next_is_explicit_padding && col < screen_width {
                            if let Some(col) = visible_cell(col, self.desired.width) {
                                self.reference_set_desired_glyph_cell(
                                    screen_row,
                                    col,
                                    ' ',
                                    attrs,
                                    base_visible,
                                    BlankErase::Explicit,
                                );
                            }
                            col = col.saturating_add(1);
                        }
                    }
                }
                preceding_wide_base_visible = glyph.wide.then_some(base_visible);
                glyph_idx += 1;
            }

            if area == GlyphArea::Text {
                let right_edge = if neovm_core::window::posn_object_extent_mode().enabled() {
                    neomacs_display_protocol::posn_object_extent::tty_text_fill_right_edge(
                        area_layout,
                        !glyph_row.glyphs[GlyphArea::RightMargin.index()].is_empty(),
                        char_width,
                        frame_origin_col,
                        screen_width,
                    )
                } else {
                    if glyph_row.glyphs[GlyphArea::RightMargin.index()].is_empty() {
                        match area_layout.placement(GlyphArea::Text) {
                            GlyphAreaPlacement::Structural(geometry) => frame_origin_col
                                .saturating_add(
                                    (geometry.bounds().right() / char_width).round() as i64
                                ),
                            GlyphAreaPlacement::FollowingPreviousArea => screen_width,
                        }
                    } else {
                        match area_layout.placement(GlyphArea::RightMargin) {
                            GlyphAreaPlacement::Structural(geometry) => frame_origin_col
                                .saturating_add((geometry.bounds().x / char_width).round() as i64),
                            GlyphAreaPlacement::FollowingPreviousArea => frame_origin_col
                                .saturating_add(
                                    (area_layout
                                        .structural_coverage()
                                        .map(|coverage| coverage.right())
                                        .unwrap_or(self.desired.width as f32)
                                        / char_width)
                                        .round() as i64,
                                ),
                        }
                    }
                };
                if col < right_edge {
                    // GNU's terminal branch of `extend_face_to_end_of_line`
                    // materializes the whole remaining text area. Without an
                    // explicit `:extend` face, its attr-filtered face LOOKS
                    // default. It is nevertheless a non-default logical face
                    // when the newline/source face was non-default, and thus
                    // fails CHAR_GLYPH_SPACE_P (the font-lock comment case).
                    let blank_erase = if glyph_row.ends_at_zv {
                        BlankErase::DefaultFace
                    } else {
                        glyphs
                            .iter()
                            .rfind(|glyph| !glyph.padding)
                            .map(|glyph| self.reference_blank_erase_for_face(glyph.face_id))
                            .unwrap_or(BlankErase::DefaultFace)
                    };
                    while col < right_edge && col < screen_width {
                        if let Some(col) = visible_cell(col, self.desired.width) {
                            // A published FaceFillItem owns the resolved
                            // background behind this matrix slot. Resolve it
                            // by semantic window/row ownership: sampling the
                            // shared desired grid would inherit a previous
                            // overlapping leaf's face, which GNU's frame-matrix
                            // copy never does.
                            let local_col = i64::try_from(col)
                                .unwrap_or(i64::MAX)
                                .saturating_sub(frame_origin_col);
                            let attrs = self.reference_blank_cell_attrs(
                                state,
                                owner,
                                glyph_row.role,
                                local_col,
                                local_row,
                            );
                            self.reference_set_desired_glyph_cell(
                                screen_row,
                                col,
                                ' ',
                                attrs,
                                false,
                                blank_erase,
                            );
                        }
                        col = col.saturating_add(1);
                    }
                }
            }
        }

        // On TTY frames GNU has one terminal cursor, positioned after
        // glyph output by `tty_set_cursor`; row cursor markers do not
        // become painted cell attributes.
    }
}

fn seeded_rif() -> TtyRif {
    let mut rif = TtyRif::new(12, 2);
    for col in 0..12 {
        let mut value = TtyCellValue {
            ch: ' ',
            attrs: CellAttrs::default(),
            blank_erase: BlankErase::DefaultFace,
            materialization: if col % 2 == 0 {
                CellMaterialization::Erased
            } else {
                CellMaterialization::Written
            },
            padding: false,
            extenders: None,
            terminal_advance: TerminalAdvance::OneColumn,
        };
        match col {
            2 => value.attrs.fg = Some(TerminalColor::Indexed(0)),
            3 => value.ch = 'X',
            4 => value.padding = true,
            5 => value.extenders = Some(Box::from("")),
            6 => value.extenders = Some(Box::from("\u{301}")),
            7 => value.blank_erase = BlankErase::Explicit,
            8 => value.terminal_advance = TerminalAdvance::TerminalResolved,
            9 => value.attrs.underline_color = Some(TerminalColor::Indexed(0)),
            10 => value.attrs.bold = true,
            11 => value.attrs.bg = Some(TerminalColor::Direct { r: 0, g: 0, b: 0 }),
            _ => {}
        }
        rif.current.cells[col] = TtyCell::new(value);
    }
    rif
}

fn assert_same_grids(actual: &TtyRif, old: &TtyRif) {
    assert_eq!(actual.desired.cells, old.desired.cells);
    assert_eq!(actual.desired.row_written, old.desired.row_written);
    assert_eq!(actual.desired.row_carried, old.desired.row_carried);
    assert_eq!(actual.current.cells, old.current.cells);
}

#[test]
fn packed_scalar_writer_matches_full_old_model_after_every_coordinate_write() {
    let mut actual = seeded_rif();
    let mut old = actual.clone();
    for (ch, padding, erase, attrs) in [
        (' ', false, BlankErase::DefaultFace, CellAttrs::default()),
        (' ', false, BlankErase::Explicit, CellAttrs::default()),
        ('X', false, BlankErase::Explicit, CellAttrs::default()),
        (' ', true, BlankErase::DefaultFace, CellAttrs::default()),
        (
            ' ',
            false,
            BlankErase::DefaultFace,
            CellAttrs {
                fg: Some(TerminalColor::Indexed(0)),
                ..CellAttrs::default()
            },
        ),
        (
            ' ',
            false,
            BlankErase::DefaultFace,
            CellAttrs {
                underline_color: Some(TerminalColor::Indexed(0)),
                ..CellAttrs::default()
            },
        ),
    ] {
        for col in 0..12 {
            actual.set_desired_packed_glyph_cell(
                0,
                col,
                PackedScalarCell {
                    ch,
                    attrs: PackedCellAttrs::new(attrs),
                    padding,
                    blank_erase: erase,
                },
            );
            old.reference_set_desired_glyph_cell(0, col, ch, attrs, padding, erase);
            assert_same_grids(&actual, &old);
        }
    }
}

fn seed_old_desired_wide(rif: &mut TtyRif) {
    for (col, padding) in [
        (0, false),
        (1, true),
        (5, false),
        (6, true),
        (7, true),
        (10, false),
        (11, true),
    ] {
        rif.desired.cells[col] = TtyCell::new(TtyCellValue {
            ch: if padding { ' ' } else { '日' },
            attrs: CellAttrs {
                fg: Some(TerminalColor::Indexed(col as u16 + 1)),
                underline: 255,
                ..CellAttrs::default()
            },
            blank_erase: BlankErase::DefaultFace,
            materialization: CellMaterialization::Erased,
            padding,
            extenders: Some(Box::from("\u{301}")),
            terminal_advance: TerminalAdvance::TerminalResolved,
        });
    }
}

#[test]
fn packed_sequential_span_edges_keep_old_desired_wide_neighbor_semantics() {
    let mut actual = seeded_rif();
    seed_old_desired_wide(&mut actual);
    let initial = actual.desired.cells.clone();
    let mut old = actual.clone();
    // Current has unrelated/narrow cells: the neutralizer must inspect desired.
    for (col, ch, padding) in [
        (1, '|', false),
        (5, 'X', false),
        (10, 'X', false),
        (0, '本', false),
        (1, ' ', true),
    ] {
        actual.set_desired_packed_glyph_cell(
            0,
            col,
            PackedScalarCell {
                ch,
                attrs: PackedCellAttrs::new(CellAttrs::default()),
                padding,
                blank_erase: BlankErase::Explicit,
            },
        );
        old.reference_set_desired_glyph_cell(
            0,
            col,
            ch,
            CellAttrs::default(),
            padding,
            BlankErase::Explicit,
        );
        assert_same_grids(&actual, &old);
        if col == 5 || col == 10 {
            for neighbor in if col == 5 {
                &[6usize, 7][..]
            } else {
                &[11usize][..]
            } {
                let cell = &actual.desired.cells[*neighbor];
                assert_eq!(cell.attrs(), initial[*neighbor].attrs());
                assert_eq!(cell.ch(), ' ');
                assert_eq!(cell.blank_erase(), BlankErase::Explicit);
                assert_eq!(cell.materialization(), CellMaterialization::Written);
                assert!(!cell.padding());
                assert_eq!(cell.extenders(), None);
                assert_eq!(cell.terminal_advance(), TerminalAdvance::OneColumn);
            }
        }
    }
}

#[test]
fn packed_grid_out_of_bounds_writes_preserve_cells_and_row_flags() {
    for (width, height) in [(0, 0), (0, 2), (3, 1)] {
        let mut actual = TtyGrid::new(width, height);
        let mut old = actual.clone();
        for (row, col) in [(0, 0), (0, width), (height, 0), (usize::MAX, usize::MAX)] {
            let attrs = CellAttrs {
                inverse: true,
                ..CellAttrs::default()
            };
            old.set_with_blank_erase(row, col, 'X', attrs, false, BlankErase::Explicit);
            actual.set_packed(
                row,
                col,
                PackedTtyCellValue {
                    ch: 'X',
                    attrs: PackedCellAttrs::new(attrs),
                    blank_erase: BlankErase::Explicit,
                    materialization: CellMaterialization::Written,
                    padding: false,
                    extenders: None,
                    terminal_advance: TerminalAdvance::OneColumn,
                },
            );
            assert_eq!(actual.cells, old.cells);
            assert_eq!(actual.row_written, old.row_written);
            assert_eq!(actual.row_carried, old.row_carried);
        }
    }
}

#[test]
fn packed_face_runs_clips_composites_and_overlapping_tails_match_frozen_raster() {
    use neomacs_display_protocol::face::Face;
    let mut state = FrameDisplayState::new(12, 2, 1.0, 1.0);
    for id in [0, 1, 2, 21] {
        let mut face = Face::new(FaceId::new(id));
        // This frozen optimization fixture's interned face21 is the
        // canonical default, unlike GNU's equal-paint nondefault slot.
        face.tty_space_identity = if id == 0 || id == 21 {
            neomacs_display_protocol::face::TtySpaceIdentity::CanonicalDefault
        } else {
            neomacs_display_protocol::face::TtySpaceIdentity::OtherRealized
        };
        face.use_default_foreground = id == 0 || id == 21;
        face.use_default_background = id == 0 || id == 21;
        face.terminal_foreground = Some(TerminalColor::Indexed(id as u16));
        face.terminal_background = Some(TerminalColor::Indexed(0));
        state.faces.insert(FaceId::new(id), face);
    }
    for (window, role, left, width, face, clip) in [
        (1, GlyphRowRole::Text, -2.0, 16.0, 1, None),
        (
            1,
            GlyphRowRole::Text,
            4.0,
            4.0,
            2,
            Some(Rect::new(5.0, 0.0, 2.0, 2.0)),
        ),
        (2, GlyphRowRole::Text, 0.0, 12.0, 21, None),
        (1, GlyphRowRole::ModeLine, 0.0, 12.0, 21, None),
    ] {
        state.face_fills.push(FaceFillItem {
            window_id: DisplayWindowId::new(window),
            row_role: role,
            clip_rect: clip,
            bounds: Rect::new(left, 0.0, width, 2.0),
            face_id: FaceId::new(face),
        });
    }
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.glyphs[GlyphArea::LeftMargin.index()].push(Glyph::char('L', FaceId::new(99), 0));
    row.glyphs[GlyphArea::Text.index()].push(Glyph::char('A', FaceId::new(1), 0));
    row.glyphs[GlyphArea::Text.index()].push(Glyph::char(' ', FaceId::new(21), 1));
    let mut composite = Glyph::char('a', FaceId::new(1), 2);
    composite.glyph_type = GlyphType::Composite { text: "ab".into() };
    row.glyphs[GlyphArea::Text.index()].push(composite);
    let mut member = Glyph::char('b', FaceId::new(2), 3);
    member.padding = true;
    row.glyphs[GlyphArea::Text.index()].push(member);
    let mut wide = Glyph::char('日', FaceId::new(1), 4);
    wide.wide = true;
    row.glyphs[GlyphArea::Text.index()].push(wide);
    row.glyphs[GlyphArea::Text.index()].push(Glyph::padding_for(FaceId::new(99), 4));
    let terminal = neovm_core::emacs_core::composite::automatic_composition_for_terminal("ꤸꥉ");
    let mut automatic = Glyph::char('ꤸ', FaceId::new(1), 6);
    automatic.glyph_type = GlyphType::AutomaticComposite {
        text: "ꤸꥉ".into(),
        terminal: terminal.into(),
    };
    row.glyphs[GlyphArea::Text.index()].push(automatic);
    let mut metadata_member = Glyph::char('ꥉ', FaceId::new(2), 7);
    metadata_member.padding = true;
    row.glyphs[GlyphArea::Text.index()].push(metadata_member);
    row.glyphs[GlyphArea::RightMargin.index()].push(Glyph::char('R', FaceId::new(21), 5));
    let layout = GlyphRowAreaLayout::unpartitioned(
        Rect::new(0.0, 0.0, 12.0, 2.0),
        Rect::new(0.0, 0.0, 12.0, 2.0),
    );
    for origin in [-2, 0, 9] {
        for owner in [
            TtyGlyphRowOwner::Window(DisplayWindowId::new(1)),
            TtyGlyphRowOwner::FrameChrome,
        ] {
            let mut actual = seeded_rif();
            seed_old_desired_wide(&mut actual);
            actual.set_faces(state.faces.clone());
            let mut old = actual.clone();
            for fill in &state.face_fills {
                actual.rasterize_face_fill(origin, 0, &state, fill, None);
                old.reference_rasterize_face_fill(origin, 0, &state, fill, None);
                assert_same_grids(&actual, &old);
            }
            for screen_row in 0..2 {
                actual.rasterize_glyph_row(
                    &state, owner, origin, 0, origin, screen_row, &row, layout, 1.0,
                );
                old.reference_rasterize_glyph_row(
                    &state, owner, origin, 0, origin, screen_row, &row, layout, 1.0,
                );
                assert_same_grids(&actual, &old);
            }
            // A later blank window owns the same coordinates, independent of earlier paint.
            let mut blank = GlyphRow::new(GlyphRowRole::Text);
            blank.ends_at_zv = true;
            actual.rasterize_glyph_row(
                &state,
                TtyGlyphRowOwner::Window(DisplayWindowId::new(2)),
                origin,
                0,
                origin,
                0,
                &blank,
                layout,
                1.0,
            );
            old.reference_rasterize_glyph_row(
                &state,
                TtyGlyphRowOwner::Window(DisplayWindowId::new(2)),
                origin,
                0,
                origin,
                0,
                &blank,
                layout,
                1.0,
            );
            assert_same_grids(&actual, &old);
        }
    }
}
