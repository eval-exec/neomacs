use super::*;

// Independent old semantic record: its derived equality is the reference,
// rather than a test that merely repeats the new bit shifts.
#[derive(Clone, Debug, PartialEq)]
struct OldCell {
    ch: char,
    attrs: CellAttrs,
    blank_erase: BlankErase,
    materialization: CellMaterialization,
    padding: bool,
    extenders: Option<Box<str>>,
    terminal_advance: TerminalAdvance,
}

impl OldCell {
    fn pack(&self) -> TtyCell {
        TtyCell::new(TtyCellValue {
            ch: self.ch,
            attrs: self.attrs,
            blank_erase: self.blank_erase,
            materialization: self.materialization,
            padding: self.padding,
            extenders: self.extenders.clone(),
            terminal_advance: self.terminal_advance,
        })
    }

    fn unpack(cell: &TtyCell) -> Self {
        Self {
            ch: cell.ch(),
            attrs: cell.attrs(),
            blank_erase: cell.blank_erase(),
            materialization: cell.materialization(),
            padding: cell.padding(),
            extenders: cell.extenders().map(Box::<str>::from),
            terminal_advance: cell.terminal_advance(),
        }
    }
}

fn baseline() -> OldCell {
    OldCell {
        ch: ' ',
        attrs: CellAttrs::default(),
        blank_erase: BlankErase::DefaultFace,
        materialization: CellMaterialization::Erased,
        padding: false,
        extenders: None,
        terminal_advance: TerminalAdvance::OneColumn,
    }
}

fn roundtrip(old: &OldCell) {
    let packed = old.pack();
    assert_eq!(OldCell::unpack(&packed), *old);
    if old.extenders.is_none() {
        assert!(
            packed.extenders.is_empty(),
            "canonical None has an empty owning payload"
        );
    }
    assert_eq!(
        packed.metadata >> 113,
        0,
        "reserved bits must be canonical zero"
    );
}

#[test]
fn default_and_debug_preserve_old_record() {
    let old = baseline();
    assert_eq!(OldCell::unpack(&TtyCell::default()), old);
    assert_eq!(
        format!("{:?}", TtyCell::default()),
        format!("{:?}", old).replacen("OldCell", "TtyCell", 1)
    );
}

#[test]
fn every_indexed_color_value_and_all_optional_color_slots_roundtrip() {
    for index in 0..=u16::MAX {
        let mut old = baseline();
        old.attrs.fg = Some(TerminalColor::Indexed(index));
        old.attrs.bg = Some(TerminalColor::Indexed(index.wrapping_add(1)));
        old.attrs.underline_color = Some(TerminalColor::Indexed(index ^ 0x55aa));
        roundtrip(&old);
    }
    let mut old = baseline();
    old.attrs.fg = Some(TerminalColor::Indexed(0));
    assert_ne!(
        old.pack(),
        baseline().pack(),
        "None differs from Indexed(0)"
    );
    let mut direct = old.clone();
    direct.attrs.fg = Some(TerminalColor::Direct { r: 0, g: 0, b: 0 });
    assert_ne!(direct.pack(), old.pack(), "direct and indexed black differ");
}

#[test]
fn direct_channels_are_independent_and_each_channel_value_roundtrips() {
    for value in 0..=u8::MAX {
        let mut old = baseline();
        old.attrs.fg = Some(TerminalColor::Direct {
            r: value,
            g: 0,
            b: 255,
        });
        old.attrs.bg = Some(TerminalColor::Direct {
            r: 255,
            g: value,
            b: 0,
        });
        old.attrs.underline_color = Some(TerminalColor::Direct {
            r: 0,
            g: 255,
            b: value,
        });
        roundtrip(&old);
        // The reverse permutation exercises all channels in every slot.
        old.attrs.fg = Some(TerminalColor::Direct {
            r: 17,
            g: value,
            b: value ^ 0xa5,
        });
        old.attrs.bg = Some(TerminalColor::Direct {
            r: value ^ 0xa5,
            g: 17,
            b: value,
        });
        old.attrs.underline_color = Some(TerminalColor::Direct {
            r: value,
            g: value ^ 0xa5,
            b: 17,
        });
        roundtrip(&old);
    }
}

#[test]
fn every_underline_value_and_style_combination_roundtrips() {
    for underline in 0..=u8::MAX {
        for styles in 0..16 {
            let mut old = baseline();
            old.attrs.underline = underline;
            old.attrs.bold = styles & 1 != 0;
            old.attrs.italic = styles & 2 != 0;
            old.attrs.strikethrough = styles & 4 != 0;
            old.attrs.inverse = styles & 8 != 0;
            roundtrip(&old);
        }
    }
}

#[test]
fn every_unicode_scalar_roundtrips_without_width_normalization() {
    for codepoint in 0..=0x10ffff {
        if let Some(ch) = char::from_u32(codepoint) {
            let mut old = baseline();
            old.ch = ch;
            roundtrip(&old);
        }
    }
}

#[test]
fn independent_provenance_flags_and_grapheme_shapes_roundtrip() {
    for flags in 0..16 {
        for cluster in [None, Some(""), Some("\u{301}"), Some("\u{200d}👩")] {
            let mut old = baseline();
            old.blank_erase = if flags & 1 != 0 {
                BlankErase::Explicit
            } else {
                BlankErase::DefaultFace
            };
            old.materialization = if flags & 2 != 0 {
                CellMaterialization::Written
            } else {
                CellMaterialization::Erased
            };
            old.padding = flags & 4 != 0;
            old.terminal_advance = if flags & 8 != 0 {
                TerminalAdvance::TerminalResolved
            } else {
                TerminalAdvance::OneColumn
            };
            old.extenders = cluster.map(Box::from);
            roundtrip(&old);
        }
    }
}

#[test]
fn physical_equality_matches_old_record_for_every_single_field_difference() {
    let base = baseline();
    let mut variants = vec![base.clone()];
    macro_rules! variant {
        ($($field:ident).+ = $value:expr) => {{
            let mut old = base.clone(); old.$($field).+ = $value; variants.push(old);
        }};
    }
    variant!(ch = '\0');
    variant!(attrs.fg = Some(TerminalColor::Indexed(0)));
    variant!(attrs.bg = Some(TerminalColor::Direct { r: 0, g: 0, b: 0 }));
    variant!(attrs.bold = true);
    variant!(attrs.italic = true);
    variant!(attrs.underline = 255);
    variant!(attrs.underline_color = Some(TerminalColor::Indexed(u16::MAX)));
    variant!(attrs.strikethrough = true);
    variant!(attrs.inverse = true);
    variant!(blank_erase = BlankErase::Explicit);
    variant!(materialization = CellMaterialization::Written);
    variant!(padding = true);
    variant!(extenders = Some(Box::from("")));
    variant!(extenders = Some(Box::from("\u{301}")));
    variant!(terminal_advance = TerminalAdvance::TerminalResolved);
    for a in &variants {
        for b in &variants {
            assert_eq!(
                a.pack() == b.pack(),
                a == b,
                "physical equality differs for {a:?}, {b:?}"
            );
        }
    }
    // Distinct allocations with equal UTF-8 bytes must compare equal.
    let mut a = base.clone();
    a.extenders = Some(Box::from("\u{301}"));
    let mut b = base;
    b.extenders = Some(String::from("\u{301}").into_boxed_str());
    assert_eq!(a.pack(), b.pack());
}

#[test]
fn each_setter_changes_only_its_old_semantic_field() {
    let mut old = baseline();
    let mut new = old.pack();
    old.ch = '🦀';
    new.set_ch(old.ch);
    assert_eq!(OldCell::unpack(&new), old);
    old.attrs = CellAttrs {
        fg: Some(TerminalColor::Indexed(65535)),
        bg: Some(TerminalColor::Direct {
            r: 255,
            g: 17,
            b: 128,
        }),
        bold: true,
        italic: true,
        underline: 255,
        underline_color: Some(TerminalColor::Indexed(0)),
        strikethrough: true,
        inverse: true,
    };
    new.set_attrs(old.attrs);
    assert_eq!(OldCell::unpack(&new), old);
    old.blank_erase = BlankErase::Explicit;
    new.set_blank_erase(old.blank_erase);
    assert_eq!(OldCell::unpack(&new), old);
    old.materialization = CellMaterialization::Written;
    new.set_materialization(old.materialization);
    assert_eq!(OldCell::unpack(&new), old);
    old.padding = true;
    new.set_padding(old.padding);
    assert_eq!(OldCell::unpack(&new), old);
    old.terminal_advance = TerminalAdvance::TerminalResolved;
    new.set_terminal_advance(old.terminal_advance);
    assert_eq!(OldCell::unpack(&new), old);
    old.extenders = Some(Box::from(""));
    new.set_extenders(old.extenders.clone());
    assert_eq!(OldCell::unpack(&new), old);
    // Clearing every field exercises masks against already-populated neighbours.
    let base = baseline();
    new.set_ch(base.ch);
    old.ch = base.ch;
    new.set_attrs(base.attrs);
    old.attrs = base.attrs;
    new.set_blank_erase(base.blank_erase);
    old.blank_erase = base.blank_erase;
    new.set_materialization(base.materialization);
    old.materialization = base.materialization;
    new.set_padding(base.padding);
    old.padding = base.padding;
    new.set_terminal_advance(base.terminal_advance);
    old.terminal_advance = base.terminal_advance;
    new.set_extenders(None);
    old.extenders = None;
    assert_eq!(OldCell::unpack(&new), old);
    assert_eq!(new, TtyCell::default());
}

#[test]
fn clone_keeps_byte_ownership_and_no_alias_to_later_setters() {
    // New explicit clone branches preserve absent versus present-empty too.
    for cluster in [None, Some("")] {
        let mut old = baseline();
        old.extenders = cluster.map(Box::from);
        let cloned = old.pack().clone();
        assert_eq!(OldCell::unpack(&cloned), old);
        if old.extenders.is_none() {
            assert!(cloned.extenders.is_empty());
        }
    }
    let mut old = baseline();
    old.extenders = Some(Box::from("\u{301}"));
    let first = old.pack();
    let mut second = first.clone();
    assert_eq!(first, second);
    second.set_extenders(Some(Box::from("\u{308}")));
    second.set_padding(true);
    assert_eq!(OldCell::unpack(&first), old);
    assert_ne!(first, second);
}

#[test]
fn content_and_painter_projections_match_old_record_and_masks() {
    // These old field-by-field expressions are independent of the packed masks.
    fn old_damage_content(a: &OldCell, b: &OldCell) -> bool {
        a.ch == b.ch
            && a.attrs == b.attrs
            && a.blank_erase == b.blank_erase
            && a.padding == b.padding
            && a.extenders == b.extenders
            && a.terminal_advance == b.terminal_advance
    }
    fn old_painter_visible(a: &OldCell, b: &OldCell) -> bool {
        a.ch == b.ch && a.attrs == b.attrs && a.padding == b.padding && a.extenders == b.extenders
    }
    let compare = |a: &OldCell, b: &OldCell| {
        let left = a.pack();
        let right = b.pack();
        assert_eq!(
            left.same_damage_content(&right),
            old_damage_content(a, b),
            "damage content differs for {a:?}, {b:?}"
        );
        assert_eq!(
            left.same_painter_visible(&right),
            old_painter_visible(a, b),
            "painter visible content differs for {a:?}, {b:?}"
        );
        assert_eq!(left.metadata & !CANONICAL_METADATA_MASK, 0);
        assert_eq!(right.metadata & !CANONICAL_METADATA_MASK, 0);
    };
    let base = baseline();
    let mut variants = vec![base.clone()];
    macro_rules! variant {
        ($($field:ident).+ = $value:expr) => {{
            let mut old = base.clone(); old.$($field).+ = $value; variants.push(old);
        }};
    }
    variant!(ch = '\0');
    variant!(attrs.fg = Some(TerminalColor::Indexed(0)));
    variant!(attrs.bg = Some(TerminalColor::Direct { r: 0, g: 0, b: 0 }));
    variant!(attrs.bold = true);
    variant!(attrs.italic = true);
    variant!(attrs.underline = 255);
    variant!(attrs.underline_color = Some(TerminalColor::Indexed(u16::MAX)));
    variant!(attrs.strikethrough = true);
    variant!(attrs.inverse = true);
    variant!(blank_erase = BlankErase::Explicit);
    variant!(materialization = CellMaterialization::Written);
    variant!(padding = true);
    variant!(extenders = Some(Box::from("")));
    variant!(extenders = Some(Box::from("\u{301}")));
    variant!(terminal_advance = TerminalAdvance::TerminalResolved);
    for a in &variants {
        for b in &variants {
            compare(a, b);
        }
    }
    let mut combinations = Vec::new();
    for flags in 0..16 {
        for cluster in [None, Some(""), Some("\u{301}"), Some("\u{200d}👩")] {
            let mut old = base.clone();
            old.blank_erase = if flags & 1 != 0 {
                BlankErase::Explicit
            } else {
                BlankErase::DefaultFace
            };
            old.materialization = if flags & 2 != 0 {
                CellMaterialization::Written
            } else {
                CellMaterialization::Erased
            };
            old.padding = flags & 4 != 0;
            old.terminal_advance = if flags & 8 != 0 {
                TerminalAdvance::TerminalResolved
            } else {
                TerminalAdvance::OneColumn
            };
            old.extenders = cluster.map(Box::from);
            combinations.push(old);
        }
    }
    for a in &combinations {
        for b in &combinations {
            compare(a, b);
        }
    }
    // Content ignores exactly materialization; visible ignores additionally
    // erase and advance. The complete scalar value has no unnamed live bits.
    let materialization = 1u128 << MATERIALIZATION;
    let painter_ignored = (1u128 << BLANK_ERASE) | materialization | (1u128 << TERMINAL_ADVANCE);
    assert_eq!(DAMAGE_CONTENT_MASK & materialization, 0);
    assert_eq!(
        DAMAGE_CONTENT_MASK | materialization,
        CANONICAL_METADATA_MASK
    );
    assert_eq!(PAINTER_VISIBLE_MASK & painter_ignored, 0);
    assert_eq!(
        PAINTER_VISIBLE_MASK | painter_ignored,
        CANONICAL_METADATA_MASK
    );
    assert_eq!(
        DAMAGE_CONTENT_MASK ^ PAINTER_VISIBLE_MASK,
        (1u128 << BLANK_ERASE) | (1u128 << TERMINAL_ADVANCE)
    );
}
