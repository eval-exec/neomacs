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
