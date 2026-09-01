use crate::display_row::width::DisplayRowCharWidthPolicy;
use crate::font::metrics::ShapedGlyph;
use crate::glyph_advance::GlyphAdvanceQuantization;
use crate::unicode::{decode_utf8, is_cluster_extender, is_regional_indicator};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplayTextRunAdvance {
    pub(crate) char_offset: usize,
    pub(crate) byte_offset: usize,
    pub(crate) advance_px: f32,
}

impl DisplayTextRunAdvance {
    pub(crate) fn new(char_offset: usize, byte_offset: usize, advance_px: f32) -> Self {
        Self {
            char_offset,
            byte_offset,
            advance_px,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplayTextRunByteAdvance {
    pub(crate) byte_offset: usize,
    pub(crate) advance_px: f32,
}

impl DisplayTextRunByteAdvance {
    pub(crate) fn new(byte_offset: usize, advance_px: f32) -> Self {
        Self {
            byte_offset,
            advance_px,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct ComplexTextRunAdvanceCache {
    start_byte_idx: usize,
    end_byte_idx: usize,
    advances: Vec<DisplayTextRunByteAdvance>,
}

impl ComplexTextRunAdvanceCache {
    fn record(
        &mut self,
        start_byte_idx: usize,
        end_byte_idx: usize,
        advances: Vec<DisplayTextRunByteAdvance>,
    ) {
        self.start_byte_idx = start_byte_idx;
        self.end_byte_idx = end_byte_idx;
        self.advances = advances;
    }

    fn contains(&self, byte_idx: usize) -> bool {
        self.start_byte_idx <= byte_idx && byte_idx < self.end_byte_idx
    }

    fn advance_for(&self, byte_idx: usize) -> Option<f32> {
        self.advances
            .iter()
            .find(|advance| advance.byte_offset == byte_idx)
            .map(|advance| advance.advance_px)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ComplexTextRunSpan {
    text: String,
    end_byte_idx: usize,
}

impl ComplexTextRunSpan {
    fn from_text_at(text: &[u8], start_byte_idx: usize, first_char: char) -> Self {
        let script = crate::composition::complex_script(first_char);
        let mut end_byte_idx = start_byte_idx;
        let mut run_text = String::new();
        while end_byte_idx < text.len() {
            let (ch, ch_len) = decode_utf8(&text[end_byte_idx..]);
            if crate::composition::complex_script(ch) == script
                || (end_byte_idx > start_byte_idx && is_cluster_extender(ch))
            {
                run_text.push(ch);
                end_byte_idx += ch_len;
            } else {
                break;
            }
        }

        Self {
            text: run_text,
            end_byte_idx,
        }
    }

    fn text(&self) -> &str {
        &self.text
    }

    fn end_byte_idx(&self) -> usize {
        self.end_byte_idx
    }
}

pub(crate) trait ComplexTextRunAdvancePolicy {
    fn text_run_measurement(&mut self, text: &str) -> DisplayTextRunMeasurement;
    fn advance_for_columns(&mut self, ch: char, columns: usize) -> f32;
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ComplexTextRunAdvanceResolver {
    cache: ComplexTextRunAdvanceCache,
}

impl ComplexTextRunAdvanceResolver {
    pub(crate) fn advance_for_char(
        &mut self,
        text: &[u8],
        byte_idx: usize,
        ch: char,
        is_cluster_continuation: bool,
        policy: &mut impl ComplexTextRunAdvancePolicy,
    ) -> f32 {
        if is_cluster_continuation {
            return 0.0;
        }

        let columns = crate::composition::base_width_cols(ch) as usize;
        if !self.cache.contains(byte_idx) {
            let run_span = ComplexTextRunSpan::from_text_at(text, byte_idx, ch);
            let measurement = policy.text_run_measurement(run_span.text());
            self.cache.record(
                byte_idx,
                run_span.end_byte_idx(),
                measurement.base_char_byte_advances(run_span.text(), byte_idx),
            );
        }

        self.cache
            .advance_for(byte_idx)
            .unwrap_or_else(|| policy.advance_for_columns(ch, columns))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DisplayTextRunMeasurement {
    PerChar,
    Measured(Vec<DisplayTextRunAdvance>),
}

/// The two metric domains GNU redisplay permits inside one source text run.
///
/// Ordinary characters take the opened font's per-glyph device metric.
/// Only a real grapheme composition or contextual-script run crosses the
/// shaping seam.  Keeping the distinction typed prevents one emoji suffix
/// from silently changing the advances of all preceding Latin characters.
#[derive(Clone, Debug, PartialEq, Eq)]
enum DisplayTextRunMeasurementSpan {
    OrdinaryChar {
        char_offset: usize,
        byte_offset: usize,
        ch: char,
    },
    ShapedSpan {
        char_offset: usize,
        byte_range: std::ops::Range<usize>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayTextRunMeasurementGeometry {
    face_char_width_px: f32,
    fallback_char_width_px: f32,
    quantization: GlyphAdvanceQuantization,
    standalone_cell_floor: bool,
}

impl DisplayTextRunMeasurementGeometry {
    pub(crate) fn new(
        face_char_width_px: f32,
        fallback_char_width_px: f32,
        quantization: GlyphAdvanceQuantization,
        standalone_cell_floor: bool,
    ) -> Self {
        Self {
            face_char_width_px,
            fallback_char_width_px,
            quantization,
            standalone_cell_floor,
        }
    }
}

/// Font adapter used by the mixed-run measurement planner.  Production binds
/// it to one realized face; tests can provide deterministic advances without
/// exposing font-system internals through the planner's interface.
pub(crate) trait DisplayTextRunAdvancePolicy {
    fn ordinary_advance_px(&mut self, ch: char) -> f32;
    fn shape_span(&mut self, text: &str) -> Vec<ShapedGlyph>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DisplayTextRunSourceChar {
    char_offset: usize,
    byte_offset: usize,
    ch: char,
}

fn measurement_spans(text: &str) -> Vec<DisplayTextRunMeasurementSpan> {
    let chars = text
        .char_indices()
        .enumerate()
        .map(
            |(char_offset, (byte_offset, ch))| DisplayTextRunSourceChar {
                char_offset,
                byte_offset,
                ch,
            },
        )
        .collect::<Vec<_>>();
    let mut spans = Vec::with_capacity(chars.len());
    let mut index = 0usize;

    while let Some(first) = chars.get(index).copied() {
        if let Some(script) = crate::composition::complex_script(first.ch) {
            let start = index;
            index += 1;
            while let Some(next) = chars.get(index) {
                if crate::composition::complex_script(next.ch) == Some(script)
                    || is_cluster_extender(next.ch)
                {
                    index += 1;
                } else {
                    break;
                }
            }
            let end_byte = chars
                .get(index)
                .map(|source| source.byte_offset)
                .unwrap_or(text.len());
            spans.push(DisplayTextRunMeasurementSpan::ShapedSpan {
                char_offset: chars[start].char_offset,
                byte_range: chars[start].byte_offset..end_byte,
            });
            continue;
        }

        let start = index;
        let mut tail = Some((first.ch, is_regional_indicator(first.ch as u32)));
        index += 1;
        while let Some(next) = chars.get(index).copied() {
            if !crate::composition::continues_cluster(next.ch, tail) {
                break;
            }
            let completes_flag_pair =
                is_regional_indicator(next.ch as u32) && matches!(tail, Some((_, true)));
            tail = Some((
                next.ch,
                is_regional_indicator(next.ch as u32) && !completes_flag_pair,
            ));
            index += 1;
        }

        if index == start + 1 {
            spans.push(DisplayTextRunMeasurementSpan::OrdinaryChar {
                char_offset: first.char_offset,
                byte_offset: first.byte_offset,
                ch: first.ch,
            });
        } else {
            let end_byte = chars
                .get(index)
                .map(|source| source.byte_offset)
                .unwrap_or(text.len());
            spans.push(DisplayTextRunMeasurementSpan::ShapedSpan {
                char_offset: first.char_offset,
                byte_range: first.byte_offset..end_byte,
            });
        }
    }

    spans
}

impl DisplayTextRunMeasurement {
    pub(crate) fn measured_advances(&self) -> Option<&[DisplayTextRunAdvance]> {
        match self {
            Self::PerChar => None,
            Self::Measured(advances) => Some(advances),
        }
    }

    pub(crate) fn base_char_byte_advances(
        &self,
        text: &str,
        base_byte_offset: usize,
    ) -> Vec<DisplayTextRunByteAdvance> {
        let Self::Measured(advances) = self else {
            return Vec::new();
        };

        advances
            .iter()
            .filter_map(|advance| {
                let c = text.get(advance.byte_offset..)?.chars().next()?;
                (!is_cluster_extender(c)).then_some(DisplayTextRunByteAdvance::new(
                    base_byte_offset + advance.byte_offset,
                    advance.advance_px,
                ))
            })
            .collect()
    }

    pub(crate) fn advance_for(&self, char_offset: usize, byte_offset: usize) -> Option<f32> {
        match self {
            Self::PerChar => None,
            // Every constructor emits advances in ascending char_offset
            // order (they iterate `char_indices`), so a binary search
            // replaces the former whole-vector scan that made row building
            // quadratic on long runs. The byte_offset equality keeps the
            // exact old match condition.
            Self::Measured(advances) => advances
                .binary_search_by(|advance| advance.char_offset.cmp(&char_offset))
                .ok()
                .map(|index| &advances[index])
                .filter(|advance| advance.byte_offset == byte_offset)
                .and_then(|advance| {
                    (advance.advance_px.is_finite() && advance.advance_px >= 0.0)
                        .then_some(advance.advance_px)
                }),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct DisplayTextRunClusterAdvances {
    advances: std::collections::HashMap<usize, f32>,
}

impl DisplayTextRunClusterAdvances {
    fn from_shaped_glyphs(text_len: usize, glyphs: impl IntoIterator<Item = ShapedGlyph>) -> Self {
        // Aggregate per-cluster advances by byte offset. Each cluster's sum
        // accumulates its glyphs in encounter order exactly as the former
        // linear-scan version did; the map only replaces the O(glyphs *
        // clusters) lookup that made shaping measurement quadratic.
        let mut advances = std::collections::HashMap::new();
        for glyph in glyphs {
            if glyph.cluster_start > text_len {
                continue;
            }
            *advances.entry(glyph.cluster_start).or_insert(0.0) += glyph.x_advance;
        }
        Self { advances }
    }

    fn advance_at(&self, byte_offset: usize) -> Option<f32> {
        self.advances.get(&byte_offset).copied()
    }
}

pub(crate) struct DisplayTextRunMeasurementPlan;

impl DisplayTextRunMeasurementPlan {
    /// Measure a heterogeneous source run with GNU's split policy: opened-font
    /// metrics for ordinary characters, shaping only for actual compositions.
    pub(crate) fn for_mixed_text(
        text: &str,
        policy: &mut impl DisplayTextRunAdvancePolicy,
        geometry: DisplayTextRunMeasurementGeometry,
    ) -> DisplayTextRunMeasurement {
        if text.is_empty() {
            return DisplayTextRunMeasurement::PerChar;
        }

        let mut advances = Vec::with_capacity(text.chars().count());
        for span in measurement_spans(text) {
            match span {
                DisplayTextRunMeasurementSpan::OrdinaryChar {
                    char_offset,
                    byte_offset,
                    ch,
                } => advances.push(DisplayTextRunAdvance::new(
                    char_offset,
                    byte_offset,
                    policy.ordinary_advance_px(ch),
                )),
                DisplayTextRunMeasurementSpan::ShapedSpan {
                    char_offset,
                    byte_range,
                } => {
                    let span_text = &text[byte_range.clone()];
                    let shaped = policy.shape_span(span_text);
                    let measured = Self::from_shaped_glyphs(
                        span_text,
                        shaped,
                        geometry.face_char_width_px,
                        geometry.fallback_char_width_px,
                        geometry.quantization,
                        geometry.standalone_cell_floor,
                    );
                    if let Some(span_advances) = measured.measured_advances() {
                        advances.extend(span_advances.iter().cloned().map(|mut advance| {
                            advance.char_offset += char_offset;
                            advance.byte_offset += byte_range.start;
                            advance
                        }));
                    } else {
                        advances.extend(span_text.char_indices().enumerate().map(
                            |(span_char_offset, (span_byte_offset, ch))| {
                                DisplayTextRunAdvance::new(
                                    char_offset + span_char_offset,
                                    byte_range.start + span_byte_offset,
                                    policy.ordinary_advance_px(ch),
                                )
                            },
                        ));
                    }
                }
            }
        }

        if advances.is_empty() {
            DisplayTextRunMeasurement::PerChar
        } else {
            DisplayTextRunMeasurement::Measured(advances)
        }
    }

    pub(crate) fn from_resolved_source_advance(
        text: &str,
        advance_px: f32,
    ) -> DisplayTextRunMeasurement {
        if text.is_empty() {
            return DisplayTextRunMeasurement::PerChar;
        }
        let advance_px = if advance_px.is_finite() && advance_px >= 0.0 {
            advance_px
        } else {
            0.0
        };
        let advances = text
            .char_indices()
            .enumerate()
            .map(|(char_offset, (byte_offset, _))| {
                DisplayTextRunAdvance::new(char_offset, byte_offset, advance_px)
            })
            .collect();
        DisplayTextRunMeasurement::Measured(advances)
    }

    #[cfg(test)]
    pub(crate) fn uniform_for_text(text: &str, advance_px: f32) -> DisplayTextRunMeasurement {
        if text.is_empty() {
            return DisplayTextRunMeasurement::PerChar;
        }
        let advance_px = if advance_px.is_finite() && advance_px >= 0.0 {
            advance_px
        } else {
            0.0
        };
        let advances = text
            .char_indices()
            .enumerate()
            .map(|(char_offset, (byte_offset, _))| {
                DisplayTextRunAdvance::new(char_offset, byte_offset, advance_px)
            })
            .collect();
        DisplayTextRunMeasurement::Measured(advances)
    }

    pub(crate) fn from_shaped_glyphs(
        text: &str,
        glyphs: impl IntoIterator<Item = ShapedGlyph>,
        face_char_width_px: f32,
        fallback_char_width_px: f32,
        quantization: GlyphAdvanceQuantization,
        standalone_cell_floor: bool,
    ) -> DisplayTextRunMeasurement {
        let cluster_advances =
            DisplayTextRunClusterAdvances::from_shaped_glyphs(text.len(), glyphs);
        let width_policy = DisplayRowCharWidthPolicy::new(fallback_char_width_px);
        let face_char_width_px = width_policy.width(face_char_width_px);
        let fallback_char_width_px = DisplayRowCharWidthPolicy::new(face_char_width_px).fallback();
        let advances = text
            .char_indices()
            .enumerate()
            .filter_map(|(char_offset, (byte_offset, ch))| {
                let measured = cluster_advances.advance_at(byte_offset)?;
                let columns = crate::composition::base_width_cols(ch);
                let columns = columns.max(1);
                // Complex-script (composed) runs take the shaped advance
                // as-is: GNU measures compositions by the shaped gstring
                // width, and joined forms are legitimately narrower than a
                // character cell. Standalone shaped chars keep the cell
                // fallback only for terminal/fixed-pitch measurement.
                let cell_floor =
                    standalone_cell_floor && crate::composition::complex_script(ch).is_none();
                let minimum = if cell_floor {
                    DisplayRowCharWidthPolicy::new(face_char_width_px).advance_for_columns(columns)
                } else {
                    1.0
                };
                let fallback = if cell_floor {
                    DisplayRowCharWidthPolicy::new(fallback_char_width_px)
                        .advance_for_columns(columns)
                } else {
                    1.0
                };
                Some(DisplayTextRunAdvance::new(
                    char_offset,
                    byte_offset,
                    quantization.resolve(Some(measured), fallback, minimum),
                ))
            })
            .collect::<Vec<_>>();

        if advances.is_empty() {
            DisplayTextRunMeasurement::PerChar
        } else {
            DisplayTextRunMeasurement::Measured(advances)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug)]
    struct FakeComplexAdvancePolicy {
        measurement: DisplayTextRunMeasurement,
        fallback_advance: f32,
        measured_runs: Vec<String>,
        fallback_requests: Vec<(char, usize)>,
    }

    impl FakeComplexAdvancePolicy {
        fn new(measurement: DisplayTextRunMeasurement, fallback_advance: f32) -> Self {
            Self {
                measurement,
                fallback_advance,
                measured_runs: Vec::new(),
                fallback_requests: Vec::new(),
            }
        }
    }

    impl ComplexTextRunAdvancePolicy for FakeComplexAdvancePolicy {
        fn text_run_measurement(&mut self, text: &str) -> DisplayTextRunMeasurement {
            self.measured_runs.push(text.to_string());
            self.measurement.clone()
        }

        fn advance_for_columns(&mut self, ch: char, columns: usize) -> f32 {
            self.fallback_requests.push((ch, columns));
            self.fallback_advance
        }
    }

    fn shaped(cluster_start: usize, x_advance: f32) -> ShapedGlyph {
        ShapedGlyph {
            font_id: fontdb::ID::dummy(),
            glyph_id: 1,
            x: 0.0,
            y: 0.0,
            x_advance,
            cluster_start,
            cluster_end: cluster_start + 1,
        }
    }

    #[test]
    fn cluster_advances_group_shaped_glyphs_by_cluster_start() {
        let advances = DisplayTextRunClusterAdvances::from_shaped_glyphs(
            "aéb".len(),
            [
                shaped(0, 3.0),
                shaped(0, 4.5),
                shaped(3, 5.0),
                shaped(99, 10.0),
            ],
        );

        assert_eq!(advances.advance_at(0), Some(7.5));
        assert_eq!(advances.advance_at(3), Some(5.0));
        assert_eq!(advances.advance_at(1), None);
        assert_eq!(advances.advance_at(99), None);
    }

    #[test]
    fn measurement_plan_builds_uniform_advances_for_text() {
        let measurement = DisplayTextRunMeasurementPlan::uniform_for_text("aé中", 5.0);

        let DisplayTextRunMeasurement::Measured(advances) = measurement else {
            panic!("non-empty text should produce uniform measured advances");
        };
        assert_eq!(
            advances
                .iter()
                .map(|advance| (advance.char_offset, advance.byte_offset, advance.advance_px))
                .collect::<Vec<_>>(),
            vec![(0, 0, 5.0), (1, 1, 5.0), (2, 3, 5.0)]
        );
    }

    #[test]
    fn complex_run_advance_cache_records_byte_scoped_advances() {
        let mut cache = ComplexTextRunAdvanceCache::default();

        cache.record(
            10,
            18,
            vec![
                DisplayTextRunByteAdvance::new(10, 7.0),
                DisplayTextRunByteAdvance::new(14, 11.0),
            ],
        );

        assert!(!cache.contains(9));
        assert!(cache.contains(10));
        assert!(cache.contains(17));
        assert!(!cache.contains(18));
        assert_eq!(cache.advance_for(10), Some(7.0));
        assert_eq!(cache.advance_for(14), Some(11.0));
        assert_eq!(cache.advance_for(12), None);
    }

    #[test]
    fn complex_text_run_span_keeps_same_script_text() {
        let text = "abc\u{0633}\u{0644}\u{0627}def".as_bytes();
        let start = "abc".len();

        let span = ComplexTextRunSpan::from_text_at(text, start, '\u{0633}');

        assert_eq!(span.text(), "\u{0633}\u{0644}\u{0627}");
        assert_eq!(span.end_byte_idx(), "abc\u{0633}\u{0644}\u{0627}".len());
    }

    #[test]
    fn complex_text_run_span_keeps_following_cluster_extenders() {
        let text = "\u{0915}\u{093C}x".as_bytes();

        let span = ComplexTextRunSpan::from_text_at(text, 0, '\u{0915}');

        assert_eq!(span.text(), "\u{0915}\u{093C}");
        assert_eq!(span.end_byte_idx(), "\u{0915}\u{093C}".len());
    }

    #[test]
    fn complex_text_run_advance_resolver_uses_cached_measured_advances() {
        let text = "\u{0633}\u{0644}x".as_bytes();
        let measurement = DisplayTextRunMeasurement::Measured(vec![
            DisplayTextRunAdvance::new(0, 0, 7.5),
            DisplayTextRunAdvance::new(1, "\u{0633}".len(), 4.0),
        ]);
        let mut policy = FakeComplexAdvancePolicy::new(measurement, 99.0);
        let mut resolver = ComplexTextRunAdvanceResolver::default();

        assert_eq!(
            resolver.advance_for_char(text, 0, '\u{0633}', false, &mut policy),
            7.5
        );
        assert_eq!(
            resolver.advance_for_char(text, "\u{0633}".len(), '\u{0644}', false, &mut policy),
            4.0
        );

        assert_eq!(policy.measured_runs, vec!["\u{0633}\u{0644}"]);
        assert!(policy.fallback_requests.is_empty());
    }

    #[test]
    fn complex_text_run_advance_resolver_falls_back_when_run_measurement_is_empty() {
        let text = "\u{0633}x".as_bytes();
        let mut policy = FakeComplexAdvancePolicy::new(DisplayTextRunMeasurement::PerChar, 12.0);
        let mut resolver = ComplexTextRunAdvanceResolver::default();

        assert_eq!(
            resolver.advance_for_char(text, 0, '\u{0633}', false, &mut policy),
            12.0
        );

        assert_eq!(policy.measured_runs, vec!["\u{0633}"]);
        assert_eq!(policy.fallback_requests, vec![('\u{0633}', 1)]);
    }

    #[test]
    fn complex_text_run_advance_resolver_zeroes_cluster_continuations() {
        let text = "\u{0633}".as_bytes();
        let measurement =
            DisplayTextRunMeasurement::Measured(vec![DisplayTextRunAdvance::new(0, 0, 7.5)]);
        let mut policy = FakeComplexAdvancePolicy::new(measurement, 99.0);
        let mut resolver = ComplexTextRunAdvanceResolver::default();

        assert_eq!(
            resolver.advance_for_char(text, 0, '\u{0633}', true, &mut policy),
            0.0
        );

        assert!(policy.measured_runs.is_empty());
        assert!(policy.fallback_requests.is_empty());
    }
}
